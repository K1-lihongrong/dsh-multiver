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
// 只放"确实会显示给用户"的文案。key 按组件/区域分组。
const MESSAGES = {
  "zh-CN": {
    "app.title": "DSH 版本管理器",
    "app.rootDir": "根目录",
    "app.toastHint": "点击查看完整内容",

    "env.title": "环境检查",
    "env.recheck": "重新检查",
    "env.pass": "通过",
    "env.fail": "未通过",
    "env.checking": "正在检查环境...",
    "env.nonCritical": "（非致命）",
    "env.openDownload": "打开下载页",

    "installed.title": "已安装版本",
    "installed.empty": "还没有安装任何版本，从下方列表安装一个吧",
    "installed.default": "默认",
    "installed.isolated": "隔离",
    "installed.sharedHome": "共享 home",
    "installed.installedAt": "安装于",
    "installed.usage": "占用",
    "installed.reused": "复用",
    "installed.exclusive": "独占",
    "installed.selectAll": "全选",
    "installed.selected": "已选",
    "installed.items": "项",
    "installed.clearSelection": "清除选择",
    "installed.batchUninstall": "批量卸载",
    "installed.batchUninstalling": "批量卸载中...",
    "installed.run": "运行",
    "installed.running": "启动中...",
    "installed.openBrowser": "浏览器打开",
    "installed.setDefault": "设为默认",
    "installed.shortcut": "桌面快捷方式",
    "installed.scanSize": "扫描占用",
    "installed.rescanSize": "重新扫描占用",
    "installed.scanning": "扫描中...",
    "installed.isolate": "隔离",
    "installed.isolatedMenu": "已隔离",
    "installed.uninstall": "卸载",
    "installed.uninstalling": "卸载中...",
    "iso.close": "关闭隔离",
    "iso.openDir": "打开隔离目录",
    "iso.rescan": "重新扫描",
    "iso.scanSize": "扫描占用大小",
    "iso.copyShared": "复制共享数据到此",
    "iso.clear": "清理隔离数据",
    "iso.copySharedConfirm": "将把共享数据（<根>/home）复制到该版本的隔离目录。\n已存在的文件不会覆盖。\n\n继续吗？",
    "iso.clearConfirm": "将删除该版本的隔离数据（保留版本本身）。\n此操作不可恢复，继续吗？",

    "remote.title": "可用版本",
    "remote.refresh": "刷新列表",
    "remote.loading": "加载中...",
    "remote.install": "安装",
    "remote.versionPlaceholder": "输入版本号，如 0.1.5",
    "remote.searchPlaceholder": "搜索版本号，如 0.1.7",
    "remote.sortDesc": "最新在前",
    "remote.sortAsc": "最早在前",
    "remote.noMatch": "没有匹配的版本",
    "remote.clickRefresh": "点击「刷新列表」从 npm 拉取所有可安装版本",
    "remote.brokenHint": "灰色版本曾被标记为无法安装（依赖已下架）：",
    "remote.clearBroken": "清除标记",

    "paths.title": "路径设置",
    "paths.clearDefault": "清除默认",
    "paths.rootDir": "数据根目录",
    "paths.rootPlaceholder": "留空则使用软件所在目录",
    "paths.apply": "应用",
    "paths.reset": "默认",
    "paths.warnSharedHome": "运行「共享 home」版本时提示（多版本混用可能错配）",
    "paths.expandAll": "展开全部目录",
    "paths.collapse": "收起",
    "paths.clickToOpen": "点击任意路径可在资源管理器中打开",
    "paths.appearance": "外观",
    "paths.theme": "主题",
    "paths.themeSystem": "跟随系统",
    "paths.themeLight": "亮色",
    "paths.themeDark": "暗色",
    "paths.language": "语言",
    "paths.langSystem": "跟随系统",
    "paths.autoMaintenance": "启动时自动维护",
    "paths.cleanup": "清理孤立缓存",
    "paths.cleaning": "清理中...",
    "paths.prune": "回收依赖仓库",
    "paths.pruning": "回收中...",
    "paths.openLogs": "打开日志目录",

    "notes.viewNotes": "查看更新说明",
    "notes.openOriginal": "在浏览器打开原文",
    "notes.suffix": "更新说明",
    "notes.clickCollapse": "点击收起",
    "notes.empty": "该版本没有提供更新说明。",
    "notes.loadFailedHint": "无法获取更新说明（可能网络不通）。可点击下方按钮在浏览器查看。",
    "notes.loadFailed": "加载失败",
  },
  "en-US": {
    "app.title": "DSH Version Manager",
    "app.rootDir": "Root",
    "app.toastHint": "Click to view full content",

    "env.title": "Environment Check",
    "env.recheck": "Re-check",
    "env.pass": "Pass",
    "env.fail": "Failed",
    "env.checking": "Checking environment...",
    "env.nonCritical": "(non-critical)",
    "env.openDownload": "Download",

    "installed.title": "Installed Versions",
    "installed.empty": "No versions installed yet. Install one from the list below.",
    "installed.default": "default",
    "installed.isolated": "isolated",
    "installed.sharedHome": "shared home",
    "installed.installedAt": "installed",
    "installed.usage": "Usage",
    "installed.reused": "shared",
    "installed.exclusive": "exclusive",
    "installed.selectAll": "Select all",
    "installed.selected": "Selected",
    "installed.items": "",
    "installed.clearSelection": "Clear",
    "installed.batchUninstall": "Uninstall selected",
    "installed.batchUninstalling": "Uninstalling...",
    "installed.run": "Run",
    "installed.running": "Starting...",
    "installed.openBrowser": "Open in browser",
    "installed.setDefault": "Set default",
    "installed.shortcut": "Desktop shortcut",
    "installed.scanSize": "Scan size",
    "installed.rescanSize": "Rescan size",
    "installed.scanning": "Scanning...",
    "installed.isolate": "Isolate",
    "installed.isolatedMenu": "Isolated",
    "installed.uninstall": "Uninstall",
    "installed.uninstalling": "Uninstalling...",
    "iso.close": "Disable isolation",
    "iso.openDir": "Open isolated directory",
    "iso.rescan": "rescan",
    "iso.scanSize": "Scan size",
    "iso.copyShared": "Copy shared data here",
    "iso.clear": "Clear isolated data",
    "iso.copySharedConfirm": "Copy shared data (<root>/home) into this version's isolated directory.\nExisting files are not overwritten.\n\nContinue?",
    "iso.clearConfirm": "Delete this version's isolated data (the version itself is kept).\nThis cannot be undone. Continue?",

    "remote.title": "Available Versions",
    "remote.refresh": "Refresh",
    "remote.loading": "Loading...",
    "remote.install": "Install",
    "remote.versionPlaceholder": "Version, e.g. 0.1.5",
    "remote.searchPlaceholder": "Search versions, e.g. 0.1.7",
    "remote.sortDesc": "Newest first",
    "remote.sortAsc": "Oldest first",
    "remote.noMatch": "No matching versions",
    "remote.clickRefresh": "Click \"Refresh\" to fetch installable versions from npm",
    "remote.brokenHint": "Grayed versions were marked uninstallable (deps unpublished):",
    "remote.clearBroken": "Clear marks",

    "paths.title": "Paths",
    "paths.clearDefault": "Clear default",
    "paths.rootDir": "Data root",
    "paths.rootPlaceholder": "Empty = directory of this app",
    "paths.apply": "Apply",
    "paths.reset": "Default",
    "paths.warnSharedHome": "Warn when running a shared-home version (multi-version mismatch risk)",
    "paths.expandAll": "Show all directories",
    "paths.collapse": "Collapse",
    "paths.clickToOpen": "Click any path to open it in the file manager",
    "paths.appearance": "Appearance",
    "paths.theme": "Theme",
    "paths.themeSystem": "System",
    "paths.themeLight": "Light",
    "paths.themeDark": "Dark",
    "paths.language": "Language",
    "paths.langSystem": "System",
    "paths.autoMaintenance": "Auto maintenance on startup",
    "paths.cleanup": "Clean orphan cache",
    "paths.cleaning": "Cleaning...",
    "paths.prune": "Prune store",
    "paths.pruning": "Pruning...",
    "paths.openLogs": "Open log directory",

    "notes.viewNotes": "View release notes",
    "notes.openOriginal": "Open original in browser",
    "notes.suffix": "release notes",
    "notes.clickCollapse": "Click to collapse",
    "notes.empty": "This version has no release notes.",
    "notes.loadFailedHint": "Failed to load release notes (network?). Use the button below to view in a browser.",
    "notes.loadFailed": "Load failed",
  },
};

/// 系统语言 → 支持的语言
function systemLang() {
  const l = (navigator.language || "zh-CN").toLowerCase();
  return l.startsWith("zh") ? "zh-CN" : "en-US";
}

/// 解析当前实际语言
function resolve(mode) {
  if (mode === "zh-CN" || mode === "en-US") return mode;
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
    if (v === "zh-CN" || v === "en-US" || v === "system") mode = v;
  } catch (_) {}
  langMode.value = mode;
  document.documentElement.setAttribute("lang", resolve(mode));
}
