import { createApp } from "vue";
import "./styles/common.css";
import { applyStored } from "./composables/useTheme.js";
import { applyStoredLang } from "./composables/useI18n.js";
import App from "./App.vue";

// 首屏渲染前应用主题与语言（防闪烁）
applyStored();
applyStoredLang();

// 启动计时起点（供 App.vue 打点，落盘到 logs/frontend.log）
window.__BOOT_T0 = performance.now();

createApp(App).mount("#app");
