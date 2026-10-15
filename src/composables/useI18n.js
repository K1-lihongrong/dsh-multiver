// 轻量 i18n（自建，零依赖）。
//
// 设计：调用约定对齐 vue-i18n（t(key) / t(key, {n})），
// 将来若需升级到 vue-i18n，**只换本文件实现，组件不用改**。
//
// - key 用点分层级（installed.title / env.node.name）
// - 缺 key 时回退：当前语言 → 中文 → key 本身（便于发现遗漏）
// - 支持 {name} 占位符插值

import { ref, computed } from "vue";

const KEY = "dsh-multiver.lang"; // "system" | "zh-CN" | "en-US"

/// 当前语言选择（响应式）
export const langMode = ref("system");

// ── 词条表 ──
// 各语言词条拆到 ./i18n/*.js，本文件只保留加载/切换逻辑（v0.2.12）。
import zhCN from "./i18n/zh-CN.js";
import zhTW from "./i18n/zh-TW.js";
import enUS from "./i18n/en-US.js";
import jaJP from "./i18n/ja-JP.js";

const MESSAGES = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  "en-US": enUS,
  "ja-JP": jaJP,
};

/// 支持的语言列表（UI 下拉用）
export const LANGS = ["zh-CN", "zh-TW", "en-US", "ja-JP"];

/// 系统语言 → 支持的语言
function systemLang() {
  const l = (navigator.language || "zh-CN").toLowerCase();
  if (l.startsWith("zh")) {
    // 繁体区（台湾 / 香港 / 澳门）→ zh-TW
    if (l.includes("tw") || l.includes("hk") || l.includes("mo") || l.includes("hant")) return "zh-TW";
    return "zh-CN";
  }
  if (l.startsWith("ja")) return "ja-JP";
  return "en-US";
}

/// 解析当前实际语言
function resolve(mode) {
  if (LANGS.includes(mode)) return mode;
  return systemLang();
}

const current = computed(() => resolve(langMode.value));

/// 翻译：t("key") 或 t("key", { name: "x" })
export function t(key, params) {
  const lang = current.value;
  let s = MESSAGES[lang]?.[key];
  if (s === undefined) s = MESSAGES["zh-CN"]?.[key]; // 回退中文
  if (s === undefined) return key;                    // 再回退 key 本身
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replace(new RegExp("\\{" + k + "\\}", "g"), String(v));
    }
  }
  return s;
}

/// 供模板直接用的响应式 t（computed 依赖 current，语言切换会重渲染）
export function useI18n() {
  return { t, langMode, setLang };
}

/// 设置语言并持久化
export function setLang(mode) {
  langMode.value = mode;
  try { localStorage.setItem(KEY, mode); } catch (_) {}
  // 同步 <html lang>（利于无障碍/字体选择）
  document.documentElement.setAttribute("lang", resolve(mode));
}

/// 首屏前调用：读偏好
export function applyStoredLang() {
  let mode = "system";
  try {
    const v = localStorage.getItem(KEY);
    if (v === "system" || LANGS.includes(v)) mode = v;
  } catch (_) {}
  langMode.value = mode;
  document.documentElement.setAttribute("lang", resolve(mode));
}
