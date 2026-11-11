<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { getState, listInstalled, setDefault, openUrl as openUrlApi, checkUpdate as checkUpdateApi, performUpdate, exitApp, openDir as openDirApi, logFrontend, takeOpenSettings, dismissRuntimePrompt } from "./api/app.js";
import { listen } from "@tauri-apps/api/event";
import EnvCheck from "./components/EnvCheck.vue";
import { useToast } from "./composables/useToast.js";
import { t } from "./composables/useI18n.js";
import PathSettings from "./components/PathSettings.vue";
import InstalledList from "./components/InstalledList.vue";
import RemoteList from "./components/RemoteList.vue";
import AppShell from "./components/AppShell.vue";
import AppStatusBanners from "./components/AppStatusBanners.vue";
import FloatingLayer from "./components/FloatingLayer.vue";
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
    getState(),
    listInstalled(),
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
    notify(await setDefault(null));
    await refresh();
  } catch (e) { notify("" + e); }
}

async function openUrl(url) {
  try { await openUrlApi(url); }
  catch (e) { notify(t("app.openLinkFailed") + "：" + e); }
}

/// 更新检查（后台；失败时展示可关闭的错误提示，不再静默当作“无更新”）
async function checkUpdate() {
  try {
    const info = await checkUpdateApi();
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
    await performUpdate(updateInfo.value.download_url, updateInfo.value.sha256 || null);
    // 后端已启动替换程序；提示用户后退出，由批处理完成替换并重启
    notify(t("update.restarting"));
    setTimeout(() => exitApp().catch(() => {}), 1500);
  } catch (e) {
    updating.value = false;
    notify(t("update.failed") + "：" + e);
  }
}

async function openDir(which) {
  try { await openDirApi(which); }
  catch (e) { notify("" + e); }
}

// 全局错误捕获：把 WebView 侧未处理异常/拒绝上报到后端落盘（frontend.log）。
// 历史踩坑：App.vue 缺 import 导致白屏、window.confirm 在 WebView2 静默失效，
// 这些前端问题过去无任何留痕，偶发时无法排查。
function reportFrontendError(msg) {
  try { logFrontend(String(msg).slice(0, 4000)).catch(() => {}); } catch (_) {}
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
  takeOpenSettings().then((s) => { if (s) scrollToSection(s); }).catch(() => {});
  listen("open-settings", (e) => {
    const p = e.payload || {};
    if (p.section) scrollToSection(p.section);
  }).then((u) => { unlistenOpenSettings = u; }).catch(() => {});

  // M3b：窗口焦点 / 可见性变化时刷新（去抖 300ms）
  window.addEventListener("focus", scheduleFocusRefresh);
  document.addEventListener("visibilitychange", onAppVisibility);


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
  logFrontend("[boot] 首屏=" + boot.toFixed(0) + "ms (refresh=" + refreshMs.toFixed(0) + "ms)").catch(() => {});
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
  try { await dismissRuntimePrompt(); } catch (_) {}
}

onUnmounted(() => {
  window.removeEventListener("error", onWindowError);
  window.removeEventListener("unhandledrejection", onUnhandledRejection);
  if (unlistenCrash) unlistenCrash();
  if (unlistenModeStart) unlistenModeStart();
  if (unlistenProgress) unlistenProgress();
  if (unlistenOpenSettings) unlistenOpenSettings();
  // M3b：移除焦点/可见性监听并清掉未触发的去抖 timer
  window.removeEventListener("focus", scheduleFocusRefresh);
  document.removeEventListener("visibilitychange", onAppVisibility);
  if (focusRefreshTimer) { clearTimeout(focusRefreshTimer); focusRefreshTimer = null; }
});
</script>

<template>
<AppShell :state="state">
  <template #launch>
    <LaunchScreen
      v-if="launching"
      :version="launchVersion"
      :stage="launchStage"
      :error-text="launchError"
      @back="cancelLaunch"
      @view-logs="openLaunchLogs"
    />
  </template>

  <AppStatusBanners
    :root-writable="rootWritable"
    :root-writable-error="rootWritableError"
    :root-dir="state?.root_dir || ''"
    :update-error="updateError"
    :update-dismissed="updateDismissed"
    :update-info="updateInfo"
    :updating="updating"
    :runtime-needs="runtimeNeeds"
    :runtime-dismissed="runtimeDismissed"
    @go-root-settings="goToRootSettings"
    @retry-check="updateError = ''; checkUpdate()"
    @dismiss-update-error="updateError = ''"
    @open-url="openUrl"
    @one-click-update="oneClickUpdate"
    @dismiss-update="updateDismissed = true"
    @prepare-runtime="prepareRuntimeFromBanner"
    @open-guide="openGuide"
    @dismiss-runtime="dismissRuntime"
  />

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

  <template #floating>
    <FloatingLayer
      :toast="toast"
      :toast-expanded="toastExpanded"
      :guide-open="guideOpen"
      @toggle-toast="toggleToast"
      @dismiss-toast="dismissToast"
      @close-guide="guideOpen = false"
      @open-url="openUrl"
    />
  </template>
</AppShell>
</template>

<!-- 全局公共样式见 src/styles/common.css（main.js 引入）；此处仅布局专属 -->
