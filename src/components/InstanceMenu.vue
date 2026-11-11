<script setup>
import { onMounted, onUnmounted, computed } from "vue";
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
function onDocClick() {
  emit("close");
}
onMounted(() => document.addEventListener("click", onDocClick));
onUnmounted(() => document.removeEventListener("click", onDocClick));
</script>

<template>
  <div class="instance-menu" @click.stop>
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
        <span v-else class="im-muted">{{ inst.id }}</span>
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
  position: absolute; top: calc(100% + 6px); right: 0; z-index: 60;
  min-width: 420px; max-width: 560px; max-height: 60vh; overflow: auto;
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
.im-muted { font-size: 11px; color: var(--text-muted); }
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
