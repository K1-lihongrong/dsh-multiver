<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import EnvCheck from "./components/EnvCheck.vue";
import { useToast } from "./composables/useToast.js";
import { t } from "./composables/useI18n.js";
import PathSettings from "./components/PathSettings.vue";
import InstalledList from "./components/InstalledList.vue";
import RemoteList from "./components/RemoteList.vue";
import RuntimeGuide from "./components/RuntimeGuide.vue";
import LaunchScreen from "./components/LaunchScreen.vue";

// v0.2.14：main.js 挂载前已查启动模式，作为初始 prop 传入 → 第一帧即启动态（不闪主界面）
const props = defineProps({
  launchMode: { type: String, default: "" },
  launchError: { type: String, default: "" },
});

const state = ref(null);
const installed = ref([]);
// 更新提醒（C1）：查到有新版则显示横幅；可关闭（本次会话内不再提示）
const updateInfo = ref(null);
const updateDismissed = ref(false);
// 更新检查失败（网络/代理/GitHub API）：与「无更新」区分，给用户可关闭提示
const updateError = ref("");

// D19：数据根可写状态。不可写时显示**不可关闭**横幅（持续状态，不是一次性事件）
const rootWritable = computed(() => state.value?.root_writable !== false);
const rootWritableError = computed(() => state.value?.root_writable_error || "");
function goToRootSettings() {
  const el = document.querySelector("#path-settings");
  if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
}

// ── 零.1 无边框窗口控制 ──
// decorations:false 后系统标题栏消失，需自绘最小化 / 最大化 / 关闭，
// 并自行处理拖动（带 4px 阈值，让双击仍然可用）。
const isMaximized = ref(false);
let appWin = null;
let appWinBroken = false;
let unlistenWindowResize = null;

function getAppWin() {
  if (appWinBroken) return null;
  if (appWin) return appWin;
  try { appWin = getCurrentWindow(); } catch (_) { appWinBroken = true; return null; }
  return appWin;
}
/// 窗口操作失败必须留下痕迹：权限缺失时静默吞错会让「按钮完全没反应」
/// 变成无从排查的现象（v0.3.6 踩过：capabilities 少声明 core:window:* 权限）。
function reportWindowError(action, e) {
  const msg = "窗口操作失败（" + action + "）: " + (e?.message || e);
  try { invoke("log_frontend", { msg: "[window] " + msg }).catch(() => {}); } catch (_) {}
  console.error(msg);
}
async function minimizeWindow() {
  try { await getAppWin()?.minimize(); } catch (e) { reportWindowError("minimize", e); }
}
async function toggleMaxWindow() {
  try { await getAppWin()?.toggleMaximize(); } catch (e) { reportWindowError("toggleMaximize", e); }
}
async function closeWindow() {
  try { await getAppWin()?.close(); } catch (e) { reportWindowError("close", e); }
}

/// 顶栏拖动：按下后位移超过 4px 才发起系统拖动，未超阈值则放行双击 / 单击。
function onTopbarMousedown(e) {
  if (e.button !== 0) return;
  if (e.target.closest(".window-controls, button, a, input, textarea, select")) return;
  const win = getAppWin();
  if (!win) return;
  const sx = e.screenX, sy = e.screenY;
  let dragging = false;
  const onMove = (ev) => {
    if (dragging) return;
    if (Math.abs(ev.screenX - sx) > 4 || Math.abs(ev.screenY - sy) > 4) {
      dragging = true;
      cleanup();
      win.startDragging().catch((e) => reportWindowError("startDragging", e));
    }
  };
  const onUp = () => cleanup();
  const cleanup = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

// ── v0.2.14 启动态（快捷方式启动 / 运行按钮共用）──
// launching：是否显示启动态；launchVersion：启动的版本号；launchStage：preparing/waiting/error
const launching = ref(!!props.launchMode);
const launchVersion = ref(props.launchMode || "");
// 首帧就带错误 → 直接进错误态，不依赖事件（单实例路径的事件可能早于监听）
const launchStage = ref(props.launchError ? "error" : "preparing");
const launchError = ref(props.launchError || "");
let unlistenProgress = null;
let unlistenModeStart = null;
let unlistenOpenSettings = null;

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

/// M3b：窗口获得焦点或从隐藏变可见时刷新一次，带 300ms 去抖。
///
/// 目的：multiver 在 dsh 内切换默认版本后，回到 multiver 窗口即能看到新状态，
/// 不必手动点刷新。去抖用于合并「focus 与 visibilitychange 同时触发」。
let focusRefreshTimer = null;
function onAppVisibility() {
  if (document.visibilityState === 'visible') scheduleFocusRefresh();
}
function scheduleFocusRefresh() {
  if (focusRefreshTimer) clearTimeout(focusRefreshTimer);
  focusRefreshTimer = setTimeout(() => {
    focusRefreshTimer = null;
    refresh().catch(() => {});
  }, 300);
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

/// 更新检查（后台；失败时展示可关闭的错误提示，不再静默当作“无更新”）
async function checkUpdate() {
  try {
    const info = await invoke("check_update");
    if (info && info.has_update) {
      updateInfo.value = info;
    } else if (info && info.error) {
      updateError.value = info.error;
    }
  } catch (e) {
    updateError.value = "" + e;
  }
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
    // R-13：快捷方式启动失败时后端带上错误文本，加载页据此显示失败原因。
    if (p.stage === "error" && p.error) launchError.value = p.error;
  }).then((u) => { unlistenProgress = u; }).catch(() => {});

  // v0.3.2：--open-settings=<section> —— 滚动到对应区块。
  // setup 阶段 emit 的事件可能早于本监听，故先主动取一次（take 语义，取后即清）。
  const scrollToSection = (section) => {
    if (section !== "paths") return;
    // 等一帧让主界面渲染完成，否则 #path-settings 还不在 DOM 里
    requestAnimationFrame(() => {
      goToRootSettings();
    });
  };
  invoke("take_open_settings").then((s) => { if (s) scrollToSection(s); }).catch(() => {});
  listen("open-settings", (e) => {
    const p = e.payload || {};
    if (p.section) scrollToSection(p.section);
  }).then((u) => { unlistenOpenSettings = u; }).catch(() => {});

  // M3b：窗口焦点 / 可见性变化时刷新（去抖 300ms）
  window.addEventListener("focus", scheduleFocusRefresh);
  document.addEventListener("visibilitychange", onAppVisibility);

  // 零.1：无边框窗口——初始化最大化状态并跟随窗口尺寸变化
  const win = getAppWin();
  if (win) {
    win.isMaximized().then((v) => { isMaximized.value = !!v; }).catch(() => {});
    win.onResized(() => {
      win.isMaximized().then((v) => { isMaximized.value = !!v; }).catch(() => {});
    }).then((u) => { unlistenWindowResize = u; }).catch(() => {});
  }

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
  if (unlistenOpenSettings) unlistenOpenSettings();
  if (unlistenWindowResize) unlistenWindowResize();
  // M3b：移除焦点/可见性监听并清掉未触发的去抖 timer
  window.removeEventListener("focus", scheduleFocusRefresh);
  document.removeEventListener("visibilitychange", onAppVisibility);
  if (focusRefreshTimer) { clearTimeout(focusRefreshTimer); focusRefreshTimer = null; }
});
</script>

<template>
<div class="app">
  <!-- v0.2.14 启动加载页 -->
  <LaunchScreen
    v-if="launching"
    :version="launchVersion"
    :stage="launchStage"
    :error-text="launchError"
    @back="cancelLaunch"
    @view-logs="openLaunchLogs"
  />

  <header class="topbar" @mousedown="onTopbarMousedown" @dblclick="toggleMaxWindow">
    <div class="brand">
      <span class="dot"></span>
      <h1>{{ t("app.title") }}</h1>
      <span class="app-version" v-if="state && state.version">v{{ state.version }}</span>
    </div>
    <div class="topbar-right">
      <div class="root-line" v-if="state">
        <span class="label">{{ t("app.rootDir") }}</span>
        <code>{{ state.root_dir }}</code>
      </div>
      <div class="window-controls">
        <button class="wc-btn" @click="minimizeWindow" :title="t('window.minimize')" :aria-label="t('window.minimize')">
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x="1" y="4.5" width="8" height="1" fill="currentColor"/></svg>
        </button>
        <button class="wc-btn" @click="toggleMaxWindow" :title="isMaximized ? t('window.restore') : t('window.maximize')" :aria-label="isMaximized ? t('window.restore') : t('window.maximize')">
          <svg v-if="!isMaximized" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1"/></svg>
          <svg v-else width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <rect x="1" y="3" width="6" height="6" fill="none" stroke="currentColor" stroke-width="1"/>
            <rect x="3" y="1" width="6" height="6" fill="none" stroke="currentColor" stroke-width="1"/>
          </svg>
        </button>
        <button class="wc-btn wc-close" @click="closeWindow" :title="t('window.close')" :aria-label="t('window.close')">
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" stroke-width="1.2"/>
            <line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" stroke-width="1.2"/>
          </svg>
        </button>
      </div>
    </div>
  </header>

  <main class="content">
    <!-- D19：数据根不可写。持续状态 → 不可关闭；根目录改好后 state 刷新即自动消失 -->
    <div class="update-banner root-unwritable-banner" v-if="!rootWritable">
      <span class="update-text">
        {{ t("root.unwritable", { path: state?.root_dir || "", err: rootWritableError }) }}
      </span>
      <span class="update-actions">
        <button class="btn small primary" @click="goToRootSettings">{{ t("root.goSettings") }}</button>
      </span>
    </div>

    <div class="update-banner update-banner-error" v-if="updateError && !updateDismissed">
      <span class="update-text">{{ t("update.checkFailed", { err: updateError }) }}</span>
      <span class="update-actions">
        <button class="btn small" @click="updateError = ''; checkUpdate()">{{ t("update.retryCheck") }}</button>
        <button class="btn small" @click="updateError = ''">×</button>
      </span>
    </div>

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
      id="path-settings"
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
  padding: 12px 22px;
  background: var(--panel-bg);
  border-bottom: 1px solid var(--panel-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
  -webkit-user-select: none;
  user-select: none;
  cursor: default;
}
.topbar-right {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-wrap: wrap;
}
.window-controls {
  display: flex;
  gap: 2px;
  align-items: center;
}
.wc-btn {
  width: 32px;
  height: 26px;
  border: none;
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
  border-radius: 4px;
  font-family: inherit;
  padding: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background-color var(--motion-fast) var(--ease-standard),
              color var(--motion-fast) var(--ease-standard),
              transform var(--motion-fast) var(--ease-standard);
}
.wc-btn:hover { background: var(--surface-3); }
.wc-btn:active { background: var(--surface-4); transform: scale(0.96); }
.wc-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.wc-close:hover { background: var(--danger, #e5484d); color: #fff; }
.wc-close:active { background: #b83b3f; color: #fff; }
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
.update-banner-error { border-color: var(--danger, #e5484d); background: rgba(229, 72, 77, 0.08); }
.root-unwritable-banner { border-color: var(--danger, #e5484d); background: rgba(229, 72, 77, 0.08); }
.root-unwritable-banner .update-text { color: var(--danger, #e5484d); }
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
  /* D22：与全站共用动效令牌 */
  transition: color var(--motion-fast) var(--ease-standard);
}
.toast-close:hover { color: var(--toast-text); }
.toast-close:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.fade-enter-active, .fade-leave-active { transition: opacity .25s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
