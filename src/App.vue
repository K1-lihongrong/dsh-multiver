<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import EnvCheck from "./components/EnvCheck.vue";
import { useToast } from "./composables/useToast.js";
import PathSettings from "./components/PathSettings.vue";
import InstalledList from "./components/InstalledList.vue";
import RemoteList from "./components/RemoteList.vue";

const state = ref(null);
const installed = ref([]);
// toast（通知）由 useToast 提供（模块级单例，与拆出的组件共享）
const { toast, toastExpanded, notify, dismissToast, toggleToast } = useToast();
const envCheckRef = ref(null);  // <EnvCheck> 组件引用（安装前调 run()）
let unlistenCrash = null;

async function refresh() {
  state.value = await invoke("get_state");
  installed.value = await invoke("list_installed");
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
  catch (e) { notify("打开链接失败：" + e); }
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
  await refresh();
  unlistenCrash = await listen("launch-crashed", (e) => {
    const p = e.payload || {};
    notify(p.message || ("版本 " + p.version + " 启动后立即退出，请查看日志。"));
  });
  await runEnvCheck();
});

onUnmounted(() => {
  window.removeEventListener("error", onWindowError);
  window.removeEventListener("unhandledrejection", onUnhandledRejection);
  if (unlistenCrash) unlistenCrash();
});
</script>

<template>
<div class="app">
  <header class="topbar">
    <div class="brand">
      <span class="dot"></span>
      <h1>DSH 版本管理器</h1>
      <span class="app-version" v-if="state && state.version">v{{ state.version }}</span>
    </div>
    <div class="root-line" v-if="state">
      <span class="label">根目录</span>
      <code>{{ state.root_dir }}</code>
    </div>
  </header>

  <main class="content">
    <EnvCheck ref="envCheckRef" @open-url="openUrl" @error="notify" />

    <InstalledList
      :installed="installed"
      @refresh="refresh"
      @open-url="openUrl"
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
        点击查看完整内容
      </div>
      <button class="toast-close" @click.stop="dismissToast">×</button>
    </div>
  </transition>

</div>
</template>

<!-- 全局公共样式见 src/styles/common.css（main.js 引入）；此处仅布局专属 -->
<style>
.app { height: 100%; display: flex; flex-direction: column; }

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
}

.content {
  flex: 1; overflow-y: auto; padding: 18px 22px 28px;
  display: flex; flex-direction: column; gap: 16px;
}

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
</style>
