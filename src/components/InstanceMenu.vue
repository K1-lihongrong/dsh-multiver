<script setup>
import { onMounted, onUnmounted, computed, nextTick, ref } from "vue";
import { t } from "../composables/useI18n.js";

const props = defineProps({
  version: { type: String, required: true },
  instances: { type: Array, default: () => [] },
  selectedTarget: { type: String, default: null },
  runningTargets: { type: Array, default: () => [] },
  useOfficialHome: { type: Boolean, default: false },
});
const emit = defineEmits([
  "close",
  "select-target",
  "run",
  "open-dir",
  "rename",
  "remove",
  "create",
]);

const sharedLabel = computed(() =>
  props.useOfficialHome
    ? t("instances.sharedOfficial")
    : t("instances.sharedManager")
);

function runningOn(id) {
  const hit = props.runningTargets.find(
    (x) => x.kind === "instance" && x.instance === id
  );
  if (hit) return hit.version;
  return props.instances.find((i) => i.id === id)?.running_on || "";
}
function sharedRunning() {
  return props.runningTargets.some(
    (x) => x.kind === "shared" && x.version === props.version
  );
}
function runLabel(id) {
  return runningOn(id) ? t("instances.focus") : t("instances.run");
}
function removeDisabled(inst) {
  return !!inst.running_on || !!runningOn(inst.id);
}
const rootEl = ref(null);
let anchorEl = null;
let resizeObserver = null;

/// 浮层用 fixed 定位，并按「左开优先、右侧兜底」夹在视口内。
/// 操作区换行到左侧时，若仍按右对齐展开，420px 菜单会伸出屏幕左边
/// （v0.4.0 实机 bug）；两侧都不够时贴边并受 max-width 限制。
function placeMenu() {
  const el = rootEl.value;
  if (!el || !anchorEl) return;
  const anchor = anchorEl.getBoundingClientRect();
  const menu = el.getBoundingClientRect();
  const margin = 8;
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  // 最小化或窗口暂时不可见时，视口可能是 0×0；此时重排会把浮层丢到左上角，
  // 恢复后再跳回。保留上一次位置，等窗口恢复（resize/visibilitychange）再算。
  if (vw < 1 || vh < 1 || menu.width < 1 || menu.height < 1) return;

  const spaceLeft = anchor.right - margin;
  const spaceRight = vw - margin - anchor.left;
  let left;
  if (menu.width <= spaceLeft) {
    // 默认：右边缘贴按钮组，向左展开。
    left = anchor.right - menu.width;
  } else if (menu.width <= spaceRight) {
    // 左边放不下：翻到右边，左边缘贴按钮组。
    left = anchor.left;
  } else {
    // 两边都放不下：在视口内居中，避免贴左显得偏；宽度由 max-width 限制。
    left = Math.max(margin, Math.round((vw - menu.width) / 2));
  }

  let top = anchor.bottom + 6;
  if (top + menu.height > vh - margin) {
    top = anchor.top - menu.height - 6;
  }
  top = Math.max(margin, Math.min(top, vh - margin - menu.height));

  el.style.left = `${Math.round(left)}px`;
  el.style.top = `${Math.round(top)}px`;
}

function onDocClick() {
  emit("close");
}
function onKeydown(e) {
  if (e.key === "Escape") emit("close");
}
function onVisibilityChange() {
  if (document.visibilityState === "visible") placeMenu();
}
onMounted(async () => {
  anchorEl = rootEl.value?.parentElement || null;
  await nextTick();
  placeMenu();
  window.addEventListener("resize", placeMenu);
  window.addEventListener("scroll", placeMenu, true);
  document.addEventListener("click", onDocClick);
  document.addEventListener("keydown", onKeydown);
  document.addEventListener("visibilitychange", onVisibilityChange);
  if (typeof ResizeObserver !== "undefined" && rootEl.value) {
    resizeObserver = new ResizeObserver(placeMenu);
    resizeObserver.observe(rootEl.value);
  }
});
onUnmounted(() => {
  window.removeEventListener("resize", placeMenu);
  window.removeEventListener("scroll", placeMenu, true);
  document.removeEventListener("click", onDocClick);
  document.removeEventListener("keydown", onKeydown);
  document.removeEventListener("visibilitychange", onVisibilityChange);
  resizeObserver?.disconnect();
});
</script>

<template>
  <div ref="rootEl" class="instance-menu" @click.stop>
    <div class="im-head">
      <span class="im-title">{{ version }}</span>
      <span class="im-sub">{{ t("instances.panelTitle") }}</span>
    </div>

    <div class="im-row shared" :class="{ picked: !selectedTarget }">
      <input
        type="radio"
        :checked="!selectedTarget"
        @change="$emit('select-target', null)"
        :aria-label="sharedLabel"
      />
      <span class="im-name">{{ sharedLabel }}</span>
      <span v-if="sharedRunning()" class="im-running">{{ t("instances.running") }}</span>
      <button class="im-act" @click="$emit('run', { version, instance: null })">
        {{ sharedRunning() ? t("instances.focus") : t("instances.run") }}
      </button>
      <button class="im-act" @click="$emit('open-dir', null)">{{ t("instances.openDir") }}</button>
    </div>

    <div class="im-sep"></div>

    <div class="im-list">
      <div
        v-for="inst in instances"
        :key="inst.id"
        class="im-row"
        :class="{ picked: selectedTarget === inst.id }"
      >
        <input
          type="radio"
          :checked="selectedTarget === inst.id"
          @change="$emit('select-target', inst.id)"
          :aria-label="inst.name"
        />
        <span class="im-name">{{ inst.name }}</span>
        <span v-if="runningOn(inst.id)" class="im-running">
          {{ t("instances.runningOn", { v: runningOn(inst.id) }) }}
        </span>
        <button class="im-act" @click="$emit('run', { version, instance: inst.id })">
          {{ runLabel(inst.id) }}
        </button>
        <button class="im-act" @click="$emit('open-dir', inst.id)">{{ t("instances.openDir") }}</button>
        <button class="im-act" @click="$emit('rename', inst)">{{ t("instances.rename") }}</button>
        <button
          class="im-act danger"
          :disabled="removeDisabled(inst)"
          :title="removeDisabled(inst) ? t('instances.removeRunning') : ''"
          @click="$emit('remove', inst)"
        >{{ t("instances.remove") }}</button>
      </div>
      <div v-if="!instances.length" class="im-empty">{{ t("instances.empty") }}</div>
    </div>

    <div class="im-foot">
      <button class="im-create" @click="$emit('create', version)">
        {{ t("instances.new") }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.instance-menu {
  position: fixed; top: 0; left: 0; right: auto; z-index: 80;
  width: max-content;
  min-width: min(420px, calc(100vw - 16px));
  max-width: min(560px, calc(100vw - 16px));
  max-height: min(60vh, calc(100vh - 16px)); overflow: auto;
  background: var(--panel-bg); border: 1px solid var(--surface-5); border-radius: 10px;
  box-shadow: 0 10px 28px var(--shadow-pop2); padding: 8px;
}
.im-head { display: flex; align-items: baseline; gap: 8px; padding: 4px 8px 8px; }
.im-title { font-weight: 600; font-size: 13px; color: var(--text); }
.im-sub { font-size: 11px; color: var(--text-muted); }
.im-row {
  display: flex; align-items: center; flex-wrap: wrap;
  gap: 6px 8px; padding: 7px 8px; border-radius: 7px;
}
.im-row.picked { background: var(--accent-weak-2); }
.im-row:hover { background: var(--surface-4); }
.im-row input { accent-color: var(--accent); }
.im-name { font-size: 13px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1 1 120px; min-width: 0; }
.im-running { font-size: 11px; color: var(--ok); white-space: nowrap; }
.im-act {
  border: 1px solid var(--border); background: var(--panel-bg); color: var(--text-2);
  border-radius: 6px; padding: 3px 7px; font-size: 11px; cursor: pointer; font-family: inherit;
}
.im-act:hover:not(:disabled) { border-color: var(--accent-border); color: var(--accent); }
.im-act:disabled { opacity: .5; cursor: not-allowed; }
.im-act.danger { color: var(--danger); }
.im-sep { height: 1px; background: var(--surface-5); margin: 6px 4px; }
.im-list { display: flex; flex-direction: column; }
.im-empty { padding: 12px 8px; color: var(--text-muted); font-size: 12px; }
.im-foot { padding: 8px 4px 2px; border-top: 1px solid var(--surface-5); margin-top: 6px; }
.im-create {
  width: 100%; border: 1px dashed var(--accent-border); background: var(--accent-weak-2);
  color: var(--accent); border-radius: 7px; padding: 7px; font-size: 12px; cursor: pointer; font-family: inherit;
}
.im-create:hover { background: var(--accent-weak); }
</style>
