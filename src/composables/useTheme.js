// 主题管理（亮色 / 暗色 / 跟随系统）。
//
// 存 localStorage（纯前端偏好）；在 main.js 里**首屏前**调用 applyStored()
// 避免主题闪烁。App 内切换用 setTheme()。

import { ref } from "vue";

const KEY = "dsh-multiver.theme"; // "system" | "light" | "dark"

/// 当前选择（响应式，供 UI 绑定）
export const themeMode = ref("system");

/// 系统当前是否偏好暗色
function systemPrefersDark() {
  return typeof window !== "undefined"
    && window.matchMedia
    && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/// 根据 mode 计算实际主题（"dark" | "light"）
function resolve(mode) {
  if (mode === "dark") return "dark";
  if (mode === "light") return "light";
  return systemPrefersDark() ? "dark" : "light";
}

/// 把实际主题写到 <html data-theme>
function apply(mode) {
  const actual = resolve(mode);
  if (actual === "dark") {
    document.documentElement.setAttribute("data-theme", "dark");
  } else {
    document.documentElement.removeAttribute("data-theme");
  }
}

/// 读取存储的偏好（默认 system）
function stored() {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch (_) {}
  return "system";
}

/// 设置主题并持久化
export function setTheme(mode) {
  themeMode.value = mode;
  try { localStorage.setItem(KEY, mode); } catch (_) {}
  apply(mode);
}

/// 首屏前调用：读偏好 + 应用（防闪烁）
export function applyStored() {
  const mode = stored();
  themeMode.value = mode;
  apply(mode);
  // 跟随系统时，系统主题变化要实时响应
  if (window.matchMedia) {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (themeMode.value === "system") apply("system");
    });
  }
}
