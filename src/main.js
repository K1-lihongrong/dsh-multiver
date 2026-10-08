import { createApp } from "vue";
import "./styles/common.css";
import { applyStored } from "./composables/useTheme.js";
import App from "./App.vue";

// 首屏渲染前应用主题（防暗色闪烁）
applyStored();

createApp(App).mount("#app");
