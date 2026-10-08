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
    "app.openLinkFailed": "打开链接失败",
    "app.launchCrashed": "版本 {v} 启动后立即退出，请查看日志。",
    "env.checkFailed": "环境检查失败",

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
    "installed.clickNotes": "点击查看该版本更新说明",
    "installed.sharedHomeTip": "使用共享 home：多版本混用可能导致插件/依赖版本错配，测试版建议开隔离",
    "installed.openBrowserTip": "在新终端启动并在系统浏览器打开",
    "installed.shortcutTip": "在桌面创建 DSH 快捷方式",
    "installed.scanTip": "统计该版本占用的磁盘空间",
    "installed.isolateTip": "开启隔离（该版本使用独立数据目录）",
    "installed.starting": "正在启动 DSH {v}，请稍候...",
    "installed.uninstallTitle": "卸载版本",
    "installed.uninstallConfirm": "确定卸载版本 {v} ?",
    "installed.uninstallingMsg": "正在卸载 {v}...",
    "installed.nothingSelected": "未选择任何项",
    "installed.batchConfirm": "确定卸载选中的 {n} 个版本？此操作不可恢复。",
    "installed.batchProgress": "正在卸载（{i}/{n}）：{name}",
    "installed.batchDone": "批量卸载完成：共 {n} 个",
    "installed.batchDoneFail": "批量卸载完成：成功 {ok} 个，失败 {fail} 个。失败详情：",
    "installed.sharedHomeTitle": "共享 home 提示",
    "installed.sharedHomeConfirm": "版本 {v} 使用【共享 home】（<根>/home）。\n\n多个版本共用同一个 home 时，插件/依赖可能相互影响，导致某些功能异常。\n\n建议对测试版本开启【数据隔离】。\n\n（可在「路径设置」里关闭此提示）\n\n继续运行吗？",
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
    "remote.brokenTip": "此版本上次安装失败（依赖已下架），点右下角可清除标记",
    "remote.clearBroken": "清除标记",
    "remote.fetchFailed": "获取可用版本失败",
    "remote.noBrokenMarks": "没有需要清除的标记",
    "remote.clearedMarks": "已清除失败标记",
    "remote.enterVersion": "请输入版本号",
    "remote.checkingEnv": "正在检查环境...",
    "remote.envFailed": "环境检查未通过，无法安装。请查看「环境检查」面板。",
    "remote.preparing": "准备中...",
    "remote.modalNetTitle": "安装失败（网络问题）",
    "remote.modalNetDesc1": "安装",
    "remote.modalNetDesc2": "时网络请求失败。可换个 npm 源重试：",
    "remote.chooseRegistry": "选择源",
    "remote.customRegistry": "或自定义源（填写后优先）",
    "remote.modalBadTitle": "此版本无法安装",
    "remote.modalBadDesc1": "安装",
    "remote.modalBadDesc2": "失败：该版本依赖的官方子包已下架（或未完整发布），任何 npm 源都无法获取。建议换用更新的版本。",
    "remote.advanced": "其他安装方式（进阶）",
    "remote.rawError": "原始错误",
    "remote.cancel": "取消",
    "remote.gotIt": "知道了",
    "remote.retryWithRegistry": "换源重试",
    "remote.ok": "好",
    "registry.official": "npm 官方源（推荐）",
    "registry.aliyun": "阿里云 npmmirror",
    "registry.tencent": "腾讯云",
    "registry.huawei": "华为云",
    "remote.sortDescTip": "当前最新在前",
    "remote.sortAscTip": "当前最早在前",
    "remote.adv1": "1. 配置官方私有源令牌（需官方授权）：",
    "remote.adv1b": "将上面两行写入",
    "remote.adv1c": "，再回本工具重试。",
    "remote.adv2": "2. 从源码构建：",
    "remote.adv2b": "源码构建不受 npm 包发布状态影响，但需自行维护版本。",

    "paths.title": "路径设置",
    "paths.clearDefault": "清除默认",
    "paths.rootDir": "数据根目录",
    "paths.rootPlaceholder": "留空则使用软件所在目录",
    "paths.apply": "应用",
    "paths.reset": "默认",
    "paths.warnSharedHome": "运行「共享 home」版本时提示（多版本混用可能错配）",
    "paths.versionsDesc": "已安装的各版本 dsh",
    "paths.homeDesc": "dsh 数据目录（相当于 ~/.dsh）",
    "paths.storeDesc": "pnpm 依赖仓库（硬链接省磁盘）",
    "paths.cacheDesc": "pnpm 下载/元数据缓存（可安全删除）",
    "paths.stateDesc": "pnpm 运行状态（可安全删除）",
    "paths.expandAll": "展开全部目录",
    "paths.collapse": "收起",
    "paths.clickToOpen": "点击任意路径可在资源管理器中打开",
    "paths.appearance": "外观与语言",
    "paths.theme": "主题",
    "paths.themeSystem": "跟随系统",
    "paths.themeLight": "亮色",
    "paths.themeDark": "暗色",
    "paths.language": "语言",
    "paths.langSystem": "跟随系统",
    "paths.autoMaintenance": "启动时自动维护",
    "paths.rule1": "· 每次启动清理孤立缓存（版本已卸载的 WebView 残留）",
    "paths.rule2": "· 距上次 ≥ 7 天时，自动回收依赖仓库（pnpm store prune）",
    "paths.lastCleanup": "上次清理孤立缓存",
    "paths.lastPrune": "上次回收依赖仓库",
    "paths.cleanup": "清理孤立缓存",
    "paths.cleaning": "清理中...",
    "paths.prune": "回收依赖仓库",
    "paths.pruning": "回收中...",
    "paths.openLogs": "打开日志目录",
    "paths.never": "从未",

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
    "app.openLinkFailed": "Failed to open link",
    "app.launchCrashed": "Version {v} exited right after launch; check the logs.",
    "env.checkFailed": "Environment check failed",

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
    "installed.clickNotes": "Click to view release notes",
    "installed.sharedHomeTip": "Uses shared home: mixed versions may mismatch plugins/deps; isolate test versions",
    "installed.openBrowserTip": "Launch in a new terminal and open in the system browser",
    "installed.shortcutTip": "Create a DSH desktop shortcut",
    "installed.scanTip": "Measure this version's disk usage",
    "installed.isolateTip": "Isolate (this version gets its own data directory)",
    "installed.starting": "Starting DSH {v}, please wait...",
    "installed.uninstallTitle": "Uninstall version",
    "installed.uninstallConfirm": "Uninstall version {v}?",
    "installed.uninstallingMsg": "Uninstalling {v}...",
    "installed.nothingSelected": "Nothing selected",
    "installed.batchConfirm": "Uninstall the {n} selected versions? This cannot be undone.",
    "installed.batchProgress": "Uninstalling ({i}/{n}): {name}",
    "installed.batchDone": "Batch uninstall done: {n} total",
    "installed.batchDoneFail": "Batch uninstall done: {ok} ok, {fail} failed. Details:",
    "installed.sharedHomeTitle": "Shared home warning",
    "installed.sharedHomeConfirm": "Version {v} uses a shared home (<root>/home).\n\nMultiple versions sharing one home may interfere (plugins/deps), causing failures.\n\nConsider isolating test versions.\n\n(Disable this warning in Paths.)\n\nContinue?",
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
    "remote.brokenHint": "Grayed versions were marked uninstallable (deps unpublished): ",
    "remote.brokenTip": "Last install failed (deps unpublished); click bottom-right to clear the mark",
    "remote.clearBroken": "Clear marks",
    "remote.fetchFailed": "Failed to fetch available versions",
    "remote.noBrokenMarks": "No marks to clear",
    "remote.clearedMarks": "Marks cleared",
    "remote.enterVersion": "Enter a version",
    "remote.checkingEnv": "Checking environment...",
    "remote.envFailed": "Environment check failed; cannot install. See the Environment Check panel.",
    "remote.preparing": "Preparing...",
    "remote.modalNetTitle": "Install failed (network)",
    "remote.modalNetDesc1": "Network error while installing",
    "remote.modalNetDesc2": ". Try another npm registry:",
    "remote.chooseRegistry": "Registry",
    "remote.customRegistry": "Or a custom registry (takes precedence)",
    "remote.modalBadTitle": "This version cannot be installed",
    "remote.modalBadDesc1": "Installing",
    "remote.modalBadDesc2": "failed: its upstream sub-packages are unpublished; no registry can fetch them. Use a newer version.",
    "remote.advanced": "Other install methods (advanced)",
    "remote.rawError": "Raw error",
    "remote.cancel": "Cancel",
    "remote.gotIt": "Got it",
    "remote.retryWithRegistry": "Retry with another registry",
    "remote.ok": "OK",
    "registry.official": "npm official (recommended)",
    "registry.aliyun": "Aliyun npmmirror",
    "registry.tencent": "Tencent Cloud",
    "registry.huawei": "Huawei Cloud",
    "remote.sortDescTip": "Currently newest first",
    "remote.sortAscTip": "Currently oldest first",
    "remote.adv1": "1. Configure the official private registry token (requires official authorization):",
    "remote.adv1b": "Write those two lines into",
    "remote.adv1c": ", then retry here.",
    "remote.adv2": "2. Build from source:",
    "remote.adv2b": "Building from source is unaffected by npm publishing status, but you must maintain the version yourself.",

    "paths.title": "Paths",
    "paths.clearDefault": "Clear default",
    "paths.rootDir": "Data root",
    "paths.rootPlaceholder": "Empty = directory of this app",
    "paths.apply": "Apply",
    "paths.reset": "Default",
    "paths.warnSharedHome": "Warn when running a shared-home version (multi-version mismatch risk)",
    "paths.versionsDesc": "Installed dsh versions",
    "paths.homeDesc": "dsh data directory (~/.dsh)",
    "paths.storeDesc": "pnpm store (hard-link dedup)",
    "paths.cacheDesc": "pnpm cache (safe to delete)",
    "paths.stateDesc": "pnpm state (safe to delete)",
    "paths.expandAll": "Show all directories",
    "paths.collapse": "Collapse",
    "paths.clickToOpen": "Click any path to open it in the file manager",
    "paths.appearance": "Appearance & Language",
    "paths.theme": "Theme",
    "paths.themeSystem": "System",
    "paths.themeLight": "Light",
    "paths.themeDark": "Dark",
    "paths.language": "Language",
    "paths.langSystem": "System",
    "paths.autoMaintenance": "Auto maintenance on startup",
    "paths.rule1": "· Clean orphan cache on every startup (leftover WebView data of removed versions)",
    "paths.rule2": "· Prune the pnpm store when last run was 7+ days ago",
    "paths.lastCleanup": "Last orphan cleanup",
    "paths.lastPrune": "Last store prune",
    "paths.cleanup": "Clean orphan cache",
    "paths.cleaning": "Cleaning...",
    "paths.prune": "Prune store",
    "paths.pruning": "Pruning...",
    "paths.openLogs": "Open log directory",
    "paths.never": "Never",

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
