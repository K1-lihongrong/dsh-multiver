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

<style>
* { box-sizing: border-box; }
html, body, #app { height: 100%; margin: 0; }
body {
  font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif;
  background: #f5f6f8;
  color: #1f2328;
  font-size: 14px;
}
.app { height: 100%; display: flex; flex-direction: column; }

.topbar {
  padding: 16px 22px;
  background: #fff;
  border-bottom: 1px solid #e6e8eb;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}
.brand { display: flex; align-items: center; gap: 10px; }
.dot {
  width: 10px; height: 10px; border-radius: 50%;
  background: #4f6ef7;
  box-shadow: 0 0 0 4px rgba(79,110,247,0.15);
}
.brand h1 { font-size: 16px; margin: 0; font-weight: 600; letter-spacing: 0.2px; }
.app-version { font-size: 11px; color: #9aa1ab; background: #f0f2f5; padding: 1px 6px; border-radius: 8px; font-weight: 500; }
.root-line { display: flex; align-items: center; gap: 8px; font-size: 12px; color: #6b7280; }
.root-line .label { color: #9aa1ab; }
.root-line code {
  background: #f2f3f5; padding: 3px 8px; border-radius: 5px;
  font-size: 12px; color: #4b5563; max-width: 420px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}

.content {
  flex: 1; overflow-y: auto; padding: 18px 22px 28px;
  display: flex; flex-direction: column; gap: 16px;
}
.panel {
  background: #fff; border: 1px solid #e6e8eb; border-radius: 12px;
  padding: 16px 18px;
}
.panel-head {
  display: flex; align-items: center; justify-content: space-between;
  margin-bottom: 12px;
}
.panel-head h2 { font-size: 14px; margin: 0; font-weight: 600; color: #374151; }
.count {
  background: #eef1ff; color: #4f6ef7; font-size: 12px;
  padding: 1px 8px; border-radius: 10px; margin-left: 8px;
}

.empty, .hint { color: #9aa1ab; font-size: 13px; padding: 8px 0; }

.ver-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.ver-item {
  display: flex; align-items: center; flex-wrap: wrap;
  gap: 6px 12px;
  padding: 10px 14px; border: 1px solid #eceef1; border-radius: 9px;
  background: #fcfcfd; transition: border-color .15s, background .15s;
}
.ver-item:hover { border-color: #d6dae0; background: #fff; }
.ver-main { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; flex: 1 1 auto; }
.ver-num { font-weight: 600; font-size: 14px; font-variant-numeric: tabular-nums; }
.ver-num-clickable { cursor: pointer; user-select: none; }
.ver-num-clickable:hover { color: #4f6ef7; }
.ver-caret { font-size: 10px; margin-left: 3px; color: #9aa1ab; }
.ver-meta { color: #9aa1ab; font-size: 11px; }
.badge {
  background: #e8f5ec; color: #2f9e5f; font-size: 11px;
  padding: 2px 8px; border-radius: 10px; font-weight: 500;
}
.badge-iso { background: #fff4e5; color: #c77d1a; }
.badge-shared { background: #eef1f5; color: #6b7280; cursor: help; }
.btn.active {
  background: #fff4e5; border-color: #f0c98a; color: #c77d1a;
}
.btn.active:hover:not(:disabled) { background: #ffedcc; border-color: #e0b46a; }
.ver-actions {
  display: flex; gap: 6px; align-items: center;
  flex-wrap: wrap; justify-content: flex-start;
}

.btn {
  border: 1px solid #dcdfe4; background: #fff; color: #374151;
  padding: 6px 14px; border-radius: 7px; font-size: 13px;
  cursor: pointer; transition: all .15s; font-family: inherit;
  white-space: nowrap; flex-shrink: 0;
}
.btn:hover:not(:disabled) { border-color: #c3c8d0; background: #f7f8fa; }
.btn:disabled { opacity: .5; cursor: not-allowed; }
.btn.primary { background: #4f6ef7; border-color: #4f6ef7; color: #fff; }
.btn.primary:hover:not(:disabled) { background: #3f5ce0; border-color: #3f5ce0; }
.btn.danger { color: #d9534f; }
.btn.danger:hover:not(:disabled) { background: #fdf2f2; border-color: #f0c0c0; }
.btn.small { padding: 4px 12px; font-size: 12px; }

.toast {
  position: fixed; bottom: 22px; left: 50%; transform: translateX(-50%);
  background: #1f2328; color: #fff; padding: 12px 40px 12px 18px;
  border-radius: 9px; font-size: 13px; max-width: 80%;
  box-shadow: 0 6px 24px rgba(0,0,0,0.18); z-index: 100;
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
  margin-top: 6px; font-size: 11px; color: #9aa1ab;
}
.toast-close {
  position: absolute; top: 8px; right: 10px;
  background: none; border: none; color: #9aa1ab;
  font-size: 18px; line-height: 1; cursor: pointer; padding: 0 4px;
}
.toast-close:hover { color: #fff; }
.fade-enter-active, .fade-leave-active { transition: opacity .25s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

::-webkit-scrollbar { width: 8px; }
::-webkit-scrollbar-thumb { background: #d6dae0; border-radius: 4px; }
::-webkit-scrollbar-thumb:hover { background: #c3c8d0; }

/* ---------- 批量操作 ---------- */
.sel-box {
  flex: 0 0 auto; width: 15px; height: 15px; margin: 0 2px 0 0;
  cursor: pointer; align-self: center;
}
.ver-item.item-selected {
  background: #f2f5ff;
  border-color: #c3cdf5;
}
.batch-bar {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  margin: 8px 0 4px; padding: 8px 10px;
  background: #f7f8fa; border: 1px solid #eef1f5; border-radius: 8px;
  font-size: 12px; color: #6b7280;
}
.batch-select-all {
  display: flex; align-items: center; gap: 6px; cursor: pointer; user-select: none;
}
.batch-select-all input { cursor: pointer; }
.batch-count { color: #4f6ef7; font-weight: 600; }
</style>
