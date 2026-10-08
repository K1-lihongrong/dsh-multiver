import { createApp } from "vue";
import "./styles/common.css";
import { applyStored } from "./composables/useTheme.js";
import { applyStoredLang } from "./composables/useI18n.js";
import App from "./App.vue";

// 首屏渲染前应用主题与语言（防闪烁）
applyStored();
applyStoredLang();

createApp(App).mount("#app");
