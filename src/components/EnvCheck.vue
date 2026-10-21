<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "../composables/useI18n.js";

// 环境自检面板：自己调 check_env、自己管状态。
// 父组件通过 ref 调用 run()（安装前检查）；错误经 open-url 事件交给父组件打开链接。
const emit = defineEmits(["open-url", "error", "runtime-status"]);

const checks = ref([]);
const checked = ref(false);
const passed = ref(false);
const loading = ref(false);
// v0.2.9：运行时状态（判断是否需要"自动准备运行时"按钮）
const rtStatus = ref(null);
const preparing = ref(false);
// v0.2.9：下载进度（来自 runtime-progress 事件）
const progStage = ref("");
const progDone = ref(0);
const progTotal = ref(null);
let unlistenProgress = null;

/// 是否缺运行时（系统与便携都没有）。
function needsRuntime() {
  const s = rtStatus.value;
  if (!s || !s.portable_supported) return false;
  return (
    (!s.system_node_ok && !s.bundled_node_ok) ||
    (!s.system_pnpm_ok && !s.bundled_pnpm_ok)
  );
}

/// 非 Windows：便携运行时不可用，只能提示用户用系统包管理器。
function needsSystemRuntime() {
  const s = rtStatus.value;
  if (!s || s.portable_supported) return false;
  return !s.system_node_ok || !s.system_pnpm_ok;
}

/// 运行环境检查，返回是否通过（致命项全 ok）。
/// full：是否做 npm 源连通（网络）检查。首次启动 false（快），重查/安装前 true。
async function run(full = true) {
  loading.value = true;
  try {
    checks.value = await invoke("check_env", { full });
    rtStatus.value = await invoke("runtime_status");
    // 把运行时状态交给父组件（避免父组件再单独调一次 runtime_status）
    emit("runtime-status", rtStatus.value);
    checked.value = true;
    passed.value = checks.value.filter((c) => c.critical).every((c) => c.ok);
    return passed.value;
  } catch (e) {
    emit("error", t("env.checkFailed") + ": " + e);
    return false;
  } finally {
    loading.value = false;
  }
}

/// 自动下载缺失的便携运行时（含进度），完成后刷新检查。
async function prepareRuntime() {
  preparing.value = true;
  progStage.value = "";
  progDone.value = 0;
  progTotal.value = null;
  try {
    await invoke("setup_runtime");
    emit("error", t("env.runtimeReady"));
    await run();
  } catch (e) {
    emit("error", t("env.runtimeFailed") + ": " + e);
  } finally {
    preparing.value = false;
    progStage.value = "";
  }
}

/// 进度百分比（0-100）；总大小未知时返回 null。
function progressPct() {
  if (!progTotal.value) return null;
  return Math.min(100, Math.round((progDone.value * 100) / progTotal.value));
}

/// 已下载 / 总大小（MB 文本）。
function mbText(bytes) {
  return (bytes / 1048576).toFixed(1) + " MB";
}

onMounted(async () => {
  unlistenProgress = await listen("runtime-progress", (e) => {
    const p = e.payload || {};
    progStage.value = p.stage || "";
    progDone.value = p.done || 0;
    progTotal.value = p.total || null;
  });
  await run(false); // 首次启动：跳过网络检查，加快首屏
});

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress();
});

defineExpose({ run, prepareRuntime });
</script>

<template>
  <section class="panel">
    <div class="panel-head">
      <h2>
        {{ t("env.title") }}
        <span class="badge-ok" v-if="checked && passed">{{ t("env.pass") }}</span>
        <span class="badge-err" v-else-if="checked">{{ t("env.fail") }}</span>
      </h2>
      <button
        v-if="checked && !passed && needsRuntime()"
        class="btn small primary"
        @click="prepareRuntime"
        :disabled="preparing"
      >{{ preparing ? t("env.preparing") : t("env.prepareRuntime") }}</button>
      <button
        v-if="checked && !passed && needsRuntime()"
        class="btn small"
        @click="emit('guide')"
      >{{ t("env.installGuide") }}</button>
      <button class="btn small" @click="run(true)" :disabled="loading">{{ t("env.recheck") }}</button>
    </div>
    <ul class="env-list" v-if="checks.length">
      <li
        v-for="c in checks"
        :key="c.name"
        class="env-item"
        :class="{ 'env-item-fail': !c.ok }"
      >
        <div class="env-row">
          <span class="env-icon" :class="c.ok ? 'ok' : 'err'">{{ c.ok ? "✓" : "✕" }}</span>
          <span class="env-name">{{ c.name_key ? t(c.name_key) : c.name }}</span>
          <span class="env-detail">{{ c.detail_key ? t(c.detail_key, c.detail_args) : c.detail }}</span>
          <span class="env-critical" v-if="!c.ok && !c.critical">{{ t("env.nonCritical") }}</span>
        </div>
        <div class="env-guide" v-if="!c.ok && c.install_hint">
          <span class="env-guide-text">{{ c.hint_key ? t(c.hint_key) : c.install_hint }}</span>
          <button
            v-if="c.install_url"
            class="btn small primary"
            @click="emit('open-url', c.install_url)"
          >{{ t("env.openDownload") }}</button>
        </div>
      </li>
    </ul>
    <div v-else class="hint">{{ t("env.checking") }}</div>
    <div v-if="checked && needsSystemRuntime()" class="hint">{{ t("env.portableUnsupported") }}</div>

    <div class="rt-progress" v-if="preparing && progStage">
      <div class="prog-line">
        <span class="stage-text">{{ progStage }}</span>
        <span class="prog-size" v-if="progTotal">{{ mbText(progDone) }} / {{ mbText(progTotal) }} ({{ progressPct() }}%)</span>
      </div>
      <div class="prog-bar">
        <div class="prog-fill" :style="{ width: (progressPct() || 0) + '%' }" :class="{ indeterminate: progressPct() === null }"></div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.env-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.env-item {
  display: flex; flex-direction: column; align-items: stretch; gap: 0;
  padding: 8px 12px; border-radius: 8px; background: var(--surface-1);
  font-size: 13px;
}
.env-icon {
  width: 18px; height: 18px; border-radius: 50%; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center;
  font-size: 11px; font-weight: 700; color: #fff;
}
.env-icon.ok { background: var(--ok-2); }
.env-icon.err { background: var(--danger-2); }
.env-name { font-weight: 600; color: var(--text-2); min-width: 90px; }
.env-detail { color: var(--text-3); font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.env-critical { color: var(--danger-2); font-size: 11px; }
.env-item-fail { background: var(--danger-weak-2); }
.env-row { display: flex; align-items: center; gap: 10px; }
.env-guide {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  margin-top: 8px; margin-left: 28px;
  padding: 8px 10px; background: var(--panel-bg); border: 1px solid var(--danger-border-2);
  border-radius: 6px; font-size: 12px; color: var(--text-3); line-height: 1.5;
}
.env-guide-text { flex: 1; min-width: 200px; }
/* .badge-ok / .badge-err 见 src/styles/common.css */

/* v0.2.9 运行时下载进度 */
.rt-progress { margin-top: 10px; padding: 8px 12px; border-radius: 8px; background: var(--surface-1); }
.prog-line { display: flex; align-items: center; justify-content: space-between; gap: 10px; font-size: 12px; margin-bottom: 6px; }
.stage-text { color: var(--text-2); font-weight: 500; }
.prog-size { color: var(--text-3); font-size: 11px; }
.prog-bar { height: 6px; border-radius: 3px; background: var(--surface-3); overflow: hidden; }
.prog-fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width .2s; }
.prog-fill.indeterminate { width: 40% !important; animation: rt-indeterminate 1.2s infinite ease-in-out; }
@keyframes rt-indeterminate {
  0% { margin-left: 0; }
  50% { margin-left: 60%; }
  100% { margin-left: 0; }
}
</style>
