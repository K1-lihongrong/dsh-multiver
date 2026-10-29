import { createApp } from "vue";
import { invoke } from "@tauri-apps/api/core";
import "./styles/common.css";
import { applyStored } from "./composables/useTheme.js";
import { applyStoredLang } from "./composables/useI18n.js";
import App from "./App.vue";

// 首屏渲染前应用主题与语言（防闪烁）
applyStored();
applyStoredLang();

// 启动计时起点（供 App.vue 打点，落盘到 logs/frontend.log）
window.__BOOT_T0 = performance.now();

// v0.2.14：挂载前先查启动模式，作为初始 prop 传入。
// 这样 Vue 第一帧就是"启动态"（不闪主界面）；窗口始终可见（不再 hide→show，省 ~900ms）。
// 查询约 60ms，期间显示 index.html 的静态骨架（见 index.html）。
const launchMode = await invoke("get_launch_mode").catch(() => null);

// 单实例转发路径下，launch_window 可能在前端挂载前就失败（同步检查），
// launch-progress:error 事件会丢失。故首帧再查一次错误状态兜底。
const launchError = launchMode ? await invoke("get_launch_error").catch(() => null) : null;

// 移除静态骨架（Vue 即将挂载）
document.getElementById("boot-skeleton")?.remove();

createApp(App, { launchMode, launchError }).mount("#app");
