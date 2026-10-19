<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import EnvCheck from "./components/EnvCheck.vue";
import { useToast } from "./composables/useToast.js";
import { t } from "./composables/useI18n.js";
import PathSettings from "./components/PathSettings.vue";
import InstalledList from "./components/InstalledList.vue";
import RemoteList from "./components/RemoteList.vue";
import RuntimeGuide from "./components/RuntimeGuide.vue";
import launchPlaceholder from "./assets/launch-placeholder.svg";

// v0.2.14：main.js 挂载前已查启动模式，作为初始 prop 传入 → 第一帧即启动态（不闪主界面）
const props = defineProps({
  launchMode: { type: String, default: "" },
});

const state = ref(null);
const installed = ref([]);
// 更新提醒（C1）：查到有新版则显示横幅；可关闭（本次会话内不再提示）
const updateInfo = ref(null);
const updateDismissed = ref(false);

// ── v0.2.14 启动态（快捷方式启动 / 运行按钮共用）──
// launching：是否显示启动态；launchVersion：启动的版本号；launchStage：preparing/waiting/error
const launching = ref(!!props.launchMode);
const launchVersion = ref(props.launchMode || "");
const launchStage = ref("preparing");
const launchError = ref("");
let unlistenProgress = null;
let unlistenModeStart = null;

/// 进入启动态（运行按钮 / 事件兜底）。
function enterLaunch(version) {
  launching.value = true;
  launchVersion.value = version || "";
  launchStage.value = "preparing";
  launchError.value = "";
}

/// 运行按钮触发启动态（父组件监听 InstalledList 的 launch 事件）
function onLaunch(payload) {
  enterLaunch(payload.version);
}
/// 运行结束：成功→恢复主界面；失败→启动态显示错误
async function onLaunchDone(payload) {
  if (payload.ok) {
    launching.value = false;
    // 运行按钮场景：启动前跳过了 refresh（若首屏即启动模式），恢复主界面前补数据
    if (!installed.value.length) {
      try { await refresh(); } catch (_) {}
    }
  } else {
    launchStage.value = "error";
    launchError.value = payload.error || "";
  }
}
/// 启动态里点「返回」：关闭启动态回主界面
function cancelLaunch() {
  launching.value = false;
  launchStage.value = "preparing";
  launchError.value = "";
}
/// 启动态里点「查看日志」：打开日志目录
function openLaunchLogs() {
  openDir("logs");
}
// toast（通知）由 useToast 提供（模块级单例，与拆出的组件共享）
const { toast, toastExpanded, notify, dismissToast, toggleToast } = useToast();
const envCheckRef = ref(null);  // <EnvCheck> 组件引用（安装前调 run()）
let unlistenCrash = null;

// v0.2.9 运行时横幅：首次启动若缺运行时且用户未忽略，则显示。
const runtimeNeeds = ref(false);       // 是否缺运行时
const runtimeDismissed = ref(false);   // 本次会话内已忽略
const guideOpen = ref(false);          // 「去官网安装」综合说明对话框

async function refresh() {
  // 两个 invoke 相互独立 → 并行，省一次往返
  const [s, inst] = await Promise.all([
    invoke("get_state"),
    invoke("list_installed"),
  ]);
  state.value = s;
  installed.value = inst;
}

/// 调 EnvCheck 组件跑一次环境检查（安装前用），返回是否通过。
async function runEnvCheck() {
  return envCheckRef.value ? envCheckRef.value.run() : false;
}

async function clearDefault() {
  try {
    notify(await invoke("set_default", { version: null }));
    await refresh();
  } catch (e) { notify("" + e); }
}

async function openUrl(url) {
  try { await invoke("open_url", { url }); }
  catch (e) { notify(t("app.openLinkFailed") + "：" + e); }
}

/// 更新检查（后台，静默失败）
async function checkUpdate() {
  try {
    const info = await invoke("check_update");
    if (info && info.has_update) updateInfo.value = info;
  } catch (_) { /* 静默：更新检查不该打扰用户 */ }
}

// v0.2.10 一键更新：下载 + 校验 + 替换 + 重启
const updating = ref(false);
async function oneClickUpdate() {
  if (!updateInfo.value || !updateInfo.value.download_url) return;
  updating.value = true;
  try {
    await invoke("perform_update", {
      url: updateInfo.value.download_url,
      sha256: updateInfo.value.sha256 || null,
    });
    // 后端已启动替换程序；提示用户后退出，由批处理完成替换并重启
    notify(t("update.restarting"));
    setTimeout(() => invoke("exit_app").catch(() => {}), 1500);
  } catch (e) {
    updating.value = false;
    notify(t("update.failed") + "：" + e);
  }
}

async function openDir(which) {
  try { await invoke("open_dir", { which }); }
  catch (e) { notify("" + e); }
}

// 全局错误捕获：把 WebView 侧未处理异常/拒绝上报到后端落盘（frontend.log）。
// 历史踩坑：App.vue 缺 import 导致白屏、window.confirm 在 WebView2 静默失效，
// 这些前端问题过去无任何留痕，偶发时无法排查。
function reportFrontendError(msg) {
  try { invoke("log_frontend", { msg: String(msg).slice(0, 4000) }).catch(() => {}); } catch (_) {}
}
function onWindowError(e) {
  reportFrontendError("error: " + (e.error?.stack || e.message || e));
}
function onUnhandledRejection(e) {
  reportFrontendError("unhandledrejection: " + (e.reason?.stack || e.reason || ""));
}

onMounted(async () => {
  window.addEventListener("error", onWindowError);
  window.addEventListener("unhandledrejection", onUnhandledRejection);
  const t0 = window.__BOOT_T0 || 0;

  // ── v0.2.14 启动态 ──
  // 启动模式已在 main.js 挂载前查过（作为 prop），首帧就是启动态——这里不再重复查。
  // 事件监听后台注册（不阻塞首屏）：launch-mode-start 仅用于极端兜底（正常走 prop）。
  listen("launch-mode-start", (e) => {
    const p = e.payload || {};
    enterLaunch(p.version);
  }).then((u) => { unlistenModeStart = u; }).catch(() => {});
  listen("launch-progress", (e) => {
    const p = e.payload || {};
    if (p.stage) launchStage.value = p.stage;
  }).then((u) => { unlistenProgress = u; }).catch(() => {});

  // 启动模式：用户看的是加载页，不需要版本列表——跳过 refresh（省 ~1s）。
  // 非启动模式：正常加载主界面数据。
  if (!props.launchMode) {
    await refresh();
  }
  const t1 = performance.now();
  // 崩溃事件监听同样不进关键路径（后台注册）
  listen("launch-crashed", (e) => {
    const p = e.payload || {};
    notify(p.message || t("app.launchCrashed", { v: p.version }));
  }).then((u) => { unlistenCrash = u; }).catch(() => {});
  // 环境检查由 <EnvCheck> 自己的 onMounted 触发，这里不再重复调用（否则启动时跑两遍）。
  // 运行时横幅改由 EnvCheck 的 runtime-status 事件驱动（见 onRuntimeStatus），不再单独调 runtime_status。
  // 更新检查放最后，且不 await 阻塞（网络慢时不影响界面可用）
  checkUpdate();
  // 启动计时打点（落盘 logs/frontend.log，供排查启动耗时）
  const boot = performance.now() - t0;
  const refreshMs = t1 - t0;
  invoke("log_frontend", {
    msg: "[boot] 首屏=" + boot.toFixed(0) + "ms (refresh=" + refreshMs.toFixed(0) + "ms)",
  }).catch(() => {});
});

/// 运行时状态（由 EnvCheck 的 runtime-status 事件传入）：缺运行时且用户未忽略 → 显示横幅。
function onRuntimeStatus(s) {
  if (!s) return;
  const missing =
    (!s.system_node_ok && !s.bundled_node_ok) ||
    (!s.system_pnpm_ok && !s.bundled_pnpm_ok);
  // 用户已在配置里忽略则不显示
  if (missing && !(state.value && state.value.runtime_prompt_dismissed)) {
    runtimeNeeds.value = true;
  }
}

/// 横幅里点「准备便携版」→ 转到环境检查面板的按钮（那里有进度条）。
function prepareRuntimeFromBanner() {
  runtimeNeeds.value = false;
  if (envCheckRef.value && envCheckRef.value.prepareRuntime) {
    envCheckRef.value.prepareRuntime();
  }
}

/// 横幅里点「去官网安装」→ 打开综合说明对话框。
function openGuide() {
  guideOpen.value = true;
}

/// 横幅里点「忽略」→ 记录到配置，不再弹。
async function dismissRuntime() {
  runtimeNeeds.value = false;
  runtimeDismissed.value = true;
  try { await invoke("dismiss_runtime_prompt"); } catch (_) {}
}

onUnmounted(() => {
  window.removeEventListener("error", onWindowError);
  window.removeEventListener("unhandledrejection", onUnhandledRejection);
  if (unlistenCrash) unlistenCrash();
  if (unlistenModeStart) unlistenModeStart();
  if (unlistenProgress) unlistenProgress();
});
</script>

<template>
<div class="app">
  <!-- v0.2.14 启动态覆盖层（窗口内容区整体切换） -->
  <div class="launch-overlay" v-if="launching">
    <img class="launch-img" :src="launchPlaceholder" alt="" />
    <div class="launch-version" v-if="launchVersion">{{ launchVersion }}</div>
    <template v-if="launchStage === 'error'">
      <div class="launch-status error">{{ t("launch.failed") }}</div>
      <div class="launch-error-text">{{ launchError }}</div>
      <div class="launch-actions">
        <button class="btn primary" @click="cancelLaunch">{{ t("launch.back") }}</button>
        <button class="btn" @click="openLaunchLogs">{{ t("launch.viewLogs") }}</button>
      </div>
    </template>
    <template v-else>
      <div class="launch-status">
        {{ launchStage === 'waiting' ? t("launch.waiting") : t("launch.starting") }}
      </div>
      <div class="launch-spinner"></div>
    </template>
  </div>

  <header class="topbar">
    <div class="brand">
      <span class="dot"></span>
      <h1>{{ t("app.title") }}</h1>
      <span class="app-version" v-if="state && state.version">v{{ state.version }}</span>
    </div>
    <div class="root-line" v-if="state">
      <span class="label">{{ t("app.rootDir") }}</span>
      <code>{{ state.root_dir }}</code>
    </div>
  </header>

  <main class="content">
    <div class="update-banner" v-if="updateInfo && !updateDismissed">
      <span class="update-text">
        {{ t("update.available", { latest: updateInfo.latest, current: updateInfo.current }) }}
      </span>
      <span class="update-actions">
        <button
          v-if="updateInfo.download_url && !updating"
          class="btn small primary"
          @click="oneClickUpdate"
        >{{ t("update.updateNow") }}</button>
        <span v-if="updating" class="update-progress">{{ t("update.updating") }}</span>
        <button class="btn small" v-if="updateInfo.release_url" @click="openUrl(updateInfo.release_url)">
          {{ t("update.viewRelease") }}
        </button>
        <button class="btn small" @click="updateDismissed = true">×</button>
      </span>
    </div>

    <div class="runtime-banner" v-if="runtimeNeeds && !runtimeDismissed">
      <span class="runtime-text">{{ t("runtime.banner") }}</span>
      <span class="runtime-actions">
        <button class="btn small primary" @click="prepareRuntimeFromBanner">{{ t("runtime.prepare") }}</button>
        <button class="btn small" @click="openGuide">{{ t("runtime.guide") }}</button>
        <button class="btn small" @click="dismissRuntime">{{ t("runtime.ignore") }}</button>
      </span>
    </div>

    <EnvCheck ref="envCheckRef" @open-url="openUrl" @error="notify" @guide="openGuide" @runtime-status="onRuntimeStatus" />

    <InstalledList
      :installed="installed"
      @refresh="refresh"
      @open-url="openUrl"
      @launch="onLaunch"
      @launch-done="onLaunchDone"
    />

    <RemoteList
      :state="state"
      :check-env="runEnvCheck"
      @refresh="refresh"
      @open-url="openUrl"
    />

    <PathSettings
      :state="state"
      :default-version="state && state.default_version"
      @refresh="refresh"
      @open-dir="openDir"
      @clear-default="clearDefault"
    />
  </main>

  <transition name="fade">
    <div
      class="toast"
      :class="{ expanded: toastExpanded, long: String(toast).length > 60 }"
      v-if="toast"
      @click="toggleToast"
    >
      <div class="toast-text">{{ toast }}</div>
      <div class="toast-hint" v-if="String(toast).length > 60 && !toastExpanded">
        {{ t("app.toastHint") }}
      </div>
      <button class="toast-close" @click.stop="dismissToast">×</button>
    </div>
  </transition>

  <RuntimeGuide v-if="guideOpen" @close="guideOpen = false" @open-url="openUrl" />

</div>
</template>

<!-- 全局公共样式见 src/styles/common.css（main.js 引入）；此处仅布局专属 -->
<style>
.app { height: 100%; display: flex; flex-direction: column; position: relative; }

.topbar {
  padding: 16px 22px;
  background: var(--panel-bg);
  border-bottom: 1px solid var(--panel-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}
.brand { display: flex; align-items: center; gap: 10px; }
.dot {
  width: 10px; height: 10px; border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 0 4px var(--accent-glow);
}
.brand h1 { font-size: 16px; margin: 0; font-weight: 600; letter-spacing: 0.2px; }
.app-version { font-size: 11px; color: var(--text-muted); background: var(--surface-3); padding: 1px 6px; border-radius: 8px; font-weight: 500; }
.root-line { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-3); }
.root-line .label { color: var(--text-muted); }
.root-line code {
  background: var(--surface-2); padding: 3px 8px; border-radius: 5px;
  font-size: 12px; color: var(--text-code); max-width: 420px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  /* 路径用等宽字体：避免日文字体把反斜杠渲染成 ¥（U+005C 的字形差异） */
  font-family: ui-monospace, Consolas, "Courier New", monospace;
}

.content {
  flex: 1; overflow-y: auto; padding: 18px 22px 28px;
  display: flex; flex-direction: column; gap: 16px;
}

.update-banner {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 10px 14px; border-radius: 9px;
  background: var(--accent-weak); border: 1px solid var(--accent-border);
  font-size: 13px; flex-wrap: wrap;
}
.update-text { color: var(--text); }
.update-actions { display: flex; gap: 8px; align-items: center; }
.update-progress { font-size: 12px; color: var(--text-3); }

.runtime-banner {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 10px 14px; border-radius: 9px;
  background: var(--warn-weak, var(--accent-weak)); border: 1px solid var(--warn-border, var(--accent-border));
  font-size: 13px; flex-wrap: wrap;
}
.runtime-text { color: var(--text); line-height: 1.5; }
.runtime-actions { display: flex; gap: 8px; align-items: center; flex-shrink: 0; }

.toast {
  position: fixed; bottom: 22px; left: 50%; transform: translateX(-50%);
  background: var(--toast-bg); color: var(--toast-text); padding: 12px 40px 12px 18px;
  border-radius: 9px; font-size: 13px; max-width: 80%;
  box-shadow: 0 6px 24px var(--shadow-pop2); z-index: 100;
  cursor: pointer; user-select: text;
}
.toast.long { max-height: 60vh; overflow-y: auto; }
.toast.expanded { max-height: 60vh; overflow-y: auto; }
.toast-text {
  white-space: pre-wrap; word-break: break-all; line-height: 1.5;
  max-height: 4.5em; overflow: hidden;
}
.toast.expanded .toast-text { max-height: none; }
.toast-hint {
  margin-top: 6px; font-size: 11px; color: var(--text-muted);
}
.toast-close {
  position: absolute; top: 8px; right: 10px;
  background: none; border: none; color: var(--text-muted);
  font-size: 18px; line-height: 1; cursor: pointer; padding: 0 4px;
}
.toast-close:hover { color: var(--toast-text); }
.fade-enter-active, .fade-leave-active { transition: opacity .25s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

/* ── v0.2.14 启动态覆盖层 ── */
.launch-overlay {
  position: absolute; inset: 0; z-index: 200;
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  gap: 14px; background: var(--panel-bg);
}
.launch-img { width: 200px; height: auto; opacity: .9; user-select: none; }
.launch-version {
  font-size: 14px; color: var(--text-3); font-weight: 600;
  letter-spacing: .4px; font-variant-numeric: tabular-nums;
}
.launch-status { font-size: 15px; color: var(--text); }
.launch-status.error { color: var(--danger, #e5484d); font-weight: 600; }
.launch-error-text {
  max-width: 70%; max-height: 30vh; overflow-y: auto; font-size: 12px;
  color: var(--text-muted); white-space: pre-wrap; word-break: break-all;
  background: var(--surface-2); padding: 10px 14px; border-radius: 8px;
}
.launch-actions { display: flex; gap: 10px; margin-top: 6px; }
.launch-spinner {
  width: 30px; height: 30px; border-radius: 50%;
  border: 3px solid var(--surface-3); border-top-color: var(--accent);
  animation: launch-spin .8s linear infinite;
}
@keyframes launch-spin { to { transform: rotate(360deg); } }
</style>
