<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";

const state = ref(null);
const installed = ref([]);
const remote = ref([]);
const remoteQuery = ref("");      // 可用版本搜索词
const remoteSortDesc = ref(true); // 排序：true=最新在前
const loading = ref(false);
const toast = ref("");
const toastExpanded = ref(false);
const installInput = ref("");
const rootInput = ref("");
const installStage = ref("");
// 换源重试弹窗
const retryOpen = ref(false);
const retryVersion = ref("");
const retryError = ref("");
const retryRegistry = ref("https://registry.npmjs.org/");
const retryCustom = ref("");
// 错误分类："network" | "private-package" | "incomplete-version" | "unknown"
const retryKind = ref("unknown");
const retryAdvanced = ref(false);
// 可选 npm 源（值 = registry url；空串 = 用 pnpm 默认）
const REGISTRIES = [
  { label: "npm 官方源（推荐）", value: "https://registry.npmjs.org/" },
  { label: "阿里云 npmmirror", value: "https://registry.npmmirror.com" },
  { label: "腾讯云", value: "https://mirrors.cloud.tencent.com/npm/" },
  { label: "华为云", value: "https://repo.huaweicloud.com/repository/npm/" },
];
const installStep = ref(0);
const installTotal = ref(0);
const installDetail = ref("");
const installFraction = ref(0);
const runningVersion = ref(""); // 正在启动的版本号（用于禁用按钮 + 显示反馈）
const uninstalling = ref("");   // 正在卸载的版本号（用于禁用按钮 + 显示反馈）
// ---- 批量操作 ----
const selected = ref(new Set());  // 已选中的版本号集合
const batchBusy = ref(false);     // 批量卸载进行中
function toggleSelect(name) {
  const s = new Set(selected.value);
  if (s.has(name)) s.delete(name); else s.add(name);
  selected.value = s;
}
function isSelected(name) {
  return selected.value.has(name);
}
/// 可用版本：按搜索词过滤 + 按排序方向排列。
const filteredRemote = computed(() => {
  const q = remoteQuery.value.trim().toLowerCase();
  let list = remote.value;
  if (q) list = list.filter((v) => v.toLowerCase().includes(q));
  // remote 本身来自 npm（默认倒序=最新在前）。升序时反转。
  if (!remoteSortDesc.value) list = [...list].reverse();
  return list;
});
/// 该版本当前是否应阻塞操作：正在被卸载，或批量卸载中且被选中。
/// 批量时一次性阻塞所有选中项（而非按顺序逐个），避免"没轮到就还能点"。
function isBlocked(name) {
  return uninstalling.value === name || (batchBusy.value && selected.value.has(name));
}
/// 全选/取消全选
function toggleSelectAll(on) {
  const s = new Set(selected.value);
  for (const it of installed.value) {
    if (on) s.add(it.version); else s.delete(it.version);
  }
  selected.value = s;
}
function clearSelection() {
  selected.value = new Set();
}
/// 批量卸载已选版本（一次确认，逐个执行，实时反馈进度）
async function batchUninstall() {
  const names = Array.from(selected.value);
  if (!names.length) return notify("未选择任何项");
  if (!(await ask("确定卸载选中的 " + names.length + " 个版本？此操作不可恢复。", { title: "批量卸载", kind: "warning" }))) return;
  batchBusy.value = true;
  let okCount = 0;
  const failed = [];
  for (let i = 0; i < names.length; i++) {
    const name = names[i];
    notify("正在卸载（" + (i + 1) + "/" + names.length + "）：" + name);
    uninstalling.value = name;  // 让该卡片显示「卸载中...」
    try {
      await invoke("uninstall_version", { version: name });
      okCount++;
    } catch (e) {
      failed.push(name + "：" + e);
    }
  }
  await refresh().catch(() => {});
  uninstalling.value = "";
  selected.value = new Set();
  batchBusy.value = false;
  if (failed.length) {
    notify("批量卸载完成：成功 " + okCount + " 个，失败 " + failed.length + " 个。\n失败详情：\n" + failed.join("\n"));
  } else {
    notify("批量卸载完成：共 " + okCount + " 个");
  }
}
const envChecks = ref([]);
const envChecked = ref(false);
const envPassed = ref(false);
let toastTimer = null;
let unlistenProgress = null;
let unlistenCrash = null;
// 「运行共享 home 版本时提示」开关（持久化到 localStorage）
const warnSharedHomeEnabled = ref(localStorage.getItem("dsh-multiver.warnSharedHome") !== "0");

function notify(msg) {
  toast.value = msg;
  toastExpanded.value = false;
  if (toastTimer) clearTimeout(toastTimer);
  // 短消息 6 秒后自动消失；长消息（含换行的错误详情）不自动消失，由用户关闭
  const isLong = String(msg).length > 60 || String(msg).includes("\n");
  if (!isLong) {
    toastTimer = setTimeout(() => { toast.value = ""; }, 6000);
  }
}

/// 各阶段的「起始百分比」与「权重百分比」（按实际耗时分配，和 ≈ 100）
/// 索引 = step - 1。写入阶段（step 6）最耗时，权重最大。
const STAGE_START = [0, 1, 2, 4, 10, 35, 90, 97];
const STAGE_WEIGHT = [1, 1, 2, 6, 25, 55, 7, 3];

/// 进度百分比（0-100），按加权阶段推进 + 阶段内 fraction 细分。
function progressPct() {
  const i = installStep.value - 1;
  if (i < 0 || i >= STAGE_START.length) return 0;
  const frac = Math.min(1, Math.max(0, installFraction.value));
  return Math.round(STAGE_START[i] + STAGE_WEIGHT[i] * frac);
}

function dismissToast() {
  if (toastTimer) clearTimeout(toastTimer);
  toast.value = "";
  toastExpanded.value = false;
}

function toggleToast() {
  toastExpanded.value = !toastExpanded.value;
  if (toastExpanded.value && toastTimer) clearTimeout(toastTimer);
}

async function refresh() {
  state.value = await invoke("get_state");
  installed.value = await invoke("list_installed");
  rootInput.value = state.value.root_dir;
}

async function runEnvCheck() {
  try {
    envChecks.value = await invoke("check_env");
    envChecked.value = true;
    envPassed.value = envChecks.value
      .filter((c) => c.critical)
      .every((c) => c.ok);
    return envPassed.value;
  } catch (e) {
    notify("环境检查失败: " + e);
    return false;
  }
}

async function loadRemote() {
  loading.value = true;
  try {
    remote.value = await invoke("list_remote");
  } catch (e) {
    notify("获取可用版本失败: " + e);
  } finally {
    loading.value = false;
  }
}

/// 错误分类（与 Rust 侧 classify_error 对应）。
function classifyError(msg) {
  const s = String(msg);
  if ((s.includes("ERR_PNPM_FETCH_404") || s.includes("Not Found - 404")) && s.includes("@deepseek-ai")) {
    return "private-package";
  }
  if (s.includes("ERR_PNPM_NO_MATCHING_VERSION") || s.includes("No matching version found")) {
    return "incomplete-version";
  }
  if (
    s.includes("网络") ||
    s.includes("ERR_PNPM_FETCH") ||
    s.includes("ETIMEDOUT") ||
    s.includes("UND_ERR") ||
    s.includes("ECONNRESET") ||
    s.includes("ENOTFOUND")
  ) {
    return "network";
  }
  return "unknown";
}

/// 该版本是否被标记为「已知安装失败」
function isBroken(v) {
  return !!(state.value && state.value.broken_versions && state.value.broken_versions.includes(v));
}

/// 清除「已知安装失败」标记
async function clearBroken() {
  if (!state.value || !state.value.broken_versions || !state.value.broken_versions.length) {
    return notify("没有需要清除的标记");
  }
  try {
    await invoke("clear_broken", { version: null });
    await refresh();
    notify("已清除失败标记");
  } catch (e) {
    notify("" + e);
  }
}

/// 点可用版本 chip：填入输入框 + 在其下方展开更新说明（再点一次收起）。
function pickVersion(v) {
  installInput.value = v;
  remoteNoteVer.value = remoteNoteVer.value === v ? null : v;
  if (remoteNoteVer.value) loadNotes(v);
}

// ===== 版本更新说明（数据源：dsh 上游 GitHub releases）=====
// tag 形如 dsh-v0.2.0-rc.2。策略：点开时实时拉取 + 内存缓存；失败静默降级为只显示「打开原文」。
const RELEASE_REPO = "deepseek-ai/deepseek-harness";
const notesCache = ref({});         // version -> { status: 'loading'|'ok'|'error', html, url, empty }
const remoteNoteVer = ref(null);    // 可用版本里当前展开说明的版本号
const installedNoteVer = ref(null); // 已安装版本里当前展开说明的版本号

function releaseUrl(v) {
  return "https://github.com/" + RELEASE_REPO + "/releases/tag/dsh-v" + v;
}

/// 消毒 GitHub release 的 HTML：只保留安全标签，去掉所有属性（a 仅保留安全 href）。
function sanitizeNotes(html) {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const ALLOWED = new Set(["H1","H2","H3","H4","H5","H6","UL","OL","LI","P","BR",
    "STRONG","EM","B","I","CODE","PRE","BLOCKQUOTE","A","HR","TT","SPAN"]);
  // 这些标签连同其内容一起删除（不提升子内容，避免 script/style 内容泄漏）
  const DROP_WITH_CONTENT = new Set(["SCRIPT","STYLE","IFRAME","OBJECT","EMBED","LINK","META","NOSCRIPT","TEMPLATE"]);
  const walk = (node) => {
    for (const child of Array.from(node.childNodes)) {
      if (child.nodeType === 1) {
        if (DROP_WITH_CONTENT.has(child.tagName)) {
          child.remove();
          continue;
        }
        if (!ALLOWED.has(child.tagName)) {
          // 非危险、但不在白名单的标签：去掉标签本身，把子内容提升到当前位置
          const frag = doc.createDocumentFragment();
          while (child.firstChild) frag.appendChild(child.firstChild);
          node.replaceChild(frag, child);
          walk(node); // 重新遍历（刚提升进来的节点）
          return;
        }
        const href = child.tagName === "A" ? child.getAttribute("href") : null;
        for (const attr of Array.from(child.attributes)) child.removeAttribute(attr.name);
        if (child.tagName === "A" && href && /^(https?:\/\/|#)/i.test(href)) {
          child.setAttribute("href", href);
          if (!href.startsWith("#")) {
            child.setAttribute("target", "_blank");
            child.setAttribute("rel", "noopener noreferrer");
          }
        }
        walk(child);
      } else if (child.nodeType !== 3) {
        child.remove();
      }
    }
  };
  walk(doc.body);
  return doc.body.innerHTML;
}

/// 拉取某版本的更新说明（带缓存；失败静默降级）。
async function loadNotes(v) {
  if (notesCache.value[v]) return;
  notesCache.value = { ...notesCache.value, [v]: { status: "loading" } };
  const url = releaseUrl(v);
  try {
    const api = "https://api.github.com/repos/" + RELEASE_REPO + "/releases/tags/dsh-v" + v;
    // html+json：让 GitHub 直接返回渲染好的 HTML（body_html），Markdown 已被转换
    const resp = await fetch(api, { headers: { Accept: "application/vnd.github.html+json" } });
    if (!resp.ok) throw new Error("HTTP " + resp.status);
    const data = await resp.json();
    const body = data && typeof data.body_html === "string" ? data.body_html : "";
    notesCache.value = { ...notesCache.value, [v]: {
      status: "ok", url, empty: !body.trim(), html: sanitizeNotes(body),
    } };
  } catch (e) {
    notesCache.value = { ...notesCache.value, [v]: { status: "error", url } };
  }
}

/// 点已安装版本的版本号：展开/收起该版本更新说明。
function toggleInstalledNote(v) {
  installedNoteVer.value = installedNoteVer.value === v ? null : v;
  if (installedNoteVer.value) loadNotes(v);
}

async function install(v) {
  const ver = (v || installInput.value).trim();
  if (!ver) return notify("请输入版本号");

  // 安装前环境检查
  loading.value = true;
  installStage.value = "正在检查环境...";
  const passed = await runEnvCheck();
  if (!passed) {
    loading.value = false;
    installStage.value = "";
    notify("环境检查未通过，无法安装。请查看「环境检查」面板。");
    return;
  }

  await doInstall(ver, null);
}

/// 执行安装（registry 为 null 时用 pnpm 默认源）。
/// 失败且疑似网络问题时，弹出「换源重试」框（不静默重试）。
async function doInstall(ver, registry) {
  loading.value = true;
  installStage.value = "准备中...";
  try {
    const msg = await invoke("install_version", {
      version: ver,
      registry: registry || null,
    });
    notify(msg);
    await refresh();
  } catch (e) {
    const msg = String(e);
    const kind = classifyError(msg);
    // 失败可能记录了 broken_versions，刷新状态让列表标灰生效
    await refresh().catch(() => {});
    retryKind.value = kind;
    retryVersion.value = ver;
    retryError.value = msg;
    retryRegistry.value = registry || "https://registry.npmjs.org/";
    retryCustom.value = "";
    retryAdvanced.value = false;
    if (kind === "network" || kind === "private-package" || kind === "incomplete-version") {
      // 这三类都弹框（换源 / 换版本 / 进阶说明）
      retryOpen.value = true;
    } else {
      notify(msg);
    }
  } finally {
    loading.value = false;
    installStage.value = "";
  }
}

/// 用户在换源框里点「重试」
async function retryInstall() {
  const reg = (retryCustom.value.trim() || retryRegistry.value).trim();
  retryOpen.value = false;
  await doInstall(retryVersion.value, reg || null);
}

async function uninstall(v) {
  if (uninstalling.value) return; // 已有版本在卸载，忽略重复点击
  if (!(await ask("确定卸载版本 " + v + " ?", { title: "卸载版本", kind: "warning" }))) return;
  uninstalling.value = v;
  notify("正在卸载 " + v + "...");
  try {
    const msg = await invoke("uninstall_version", { version: v });
    notify(msg);
    await refresh();
  } catch (e) {
    notify("" + e);
  } finally {
    uninstalling.value = "";
  }
}

async function setDefault(v) {
  try {
    notify(await invoke("set_default", { version: v }));
    await refresh();
  } catch (e) { notify("" + e); }
}

async function clearDefault() {
  try {
    notify(await invoke("set_default", { version: null }));
    await refresh();
  } catch (e) { notify("" + e); }
}

/// 运行非隔离（共享 home）版本前的提示。返回 false 表示用户取消。
/// 用 Tauri 原生对话框（window.confirm 在 Tauri WebView 中不可用，会静默返回 false）。
async function confirmSharedHome(v) {
  if (!warnSharedHomeEnabled.value) return true;
  const ver = installed.value.find((x) => x.version === v);
  if (!ver || ver.isolated) return true; // 隔离版本无此风险
  return await ask(
    "版本 " + v + " 使用【共享 home】（<根>/home）。\n\n" +
    "多个版本共用同一个 home 时，插件/依赖可能相互影响，\n" +
    "导致某些功能异常（例如 0.1.7 的「在文件管理器中打开」失效会连累其它版本）。\n\n" +
    "建议对测试版本开启【数据隔离】。\n\n" +
    "（可在「路径设置」里关闭此提示）\n\n" +
    "继续运行吗？",
    { title: "共享 home 提示", kind: "warning" }
  );
}

/// 切换提示开关并持久化
function setWarnSharedHome(val) {
  warnSharedHomeEnabled.value = val;
  localStorage.setItem("dsh-multiver.warnSharedHome", val ? "1" : "0");
}

async function run(v) {
  if (!(await confirmSharedHome(v))) return;
  if (runningVersion.value) return; // 已有版本在启动，忽略重复点击
  runningVersion.value = v;
  notify("正在启动 DSH " + v + "，请稍候...");
  try {
    notify(await invoke("run_version", { version: v }));
  } catch (e) { notify("" + e); }
  finally { runningVersion.value = ""; }
}

async function createShortcut(v) {
  try {
    notify(await invoke("create_shortcut", { version: v }));
  } catch (e) { notify("" + e); }
}

async function openInBrowser(v) {
  if (!(await confirmSharedHome(v))) return;
  try {
    notify(await invoke("open_in_browser", { version: v }));
  } catch (e) { notify("" + e); }
}

async function toggleIsolated(v, current) {
  try {
    notify(await invoke("set_isolated", { version: v, isolated: !current }));
    await refresh();
  } catch (e) { notify("" + e); }
}

// ===== 隔离管理菜单 =====
const openMenu = ref(null);       // 当前展开菜单的版本号
const scanning = ref(null);       // 正在扫描的版本号（隔离数据）
const sizes = ref({});            // 版本号 -> 隔离数据字节数
const scanningVer = ref(null);    // 正在扫描"整版本占用"的版本号
const verSizes = ref({});         // 版本号 -> { total, shared_size, shared_count, exclusive_size }

async function scanVersionSize(v) {
  scanningVer.value = v;
  try {
    const info = await invoke("scan_version_size", { version: v });
    verSizes.value = { ...verSizes.value, [v]: info };
  } catch (e) {
    notify("" + e);
  } finally {
    scanningVer.value = null;
  }
}

function toggleMenu(v) {
  openMenu.value = openMenu.value === v ? null : v;
}

function closeMenu() {
  openMenu.value = null;
}

async function openIsolatedDir(v) {
  closeMenu();
  try { await invoke("open_isolated_dir", { version: v }); }
  catch (e) { notify("" + e); }
}

async function scanSize(v) {
  scanning.value = v;
  try {
    const bytes = await invoke("scan_isolated_size", { version: v });
    sizes.value = { ...sizes.value, [v]: bytes };
  } catch (e) {
    notify("" + e);
  } finally {
    scanning.value = null;
  }
}

function fmtSize(bytes) {
  if (bytes == null) return "";
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  if (bytes < 1024 * 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
  return (bytes / 1024 / 1024 / 1024).toFixed(2) + " GB";
}

async function copyShared(v) {
  closeMenu();
  if (!(await ask("将把共享数据（<根>/home）复制到该版本的隔离目录。\n已存在的文件不会覆盖。\n\n继续吗？", { title: "复制共享数据", kind: "warning" }))) return;
  try {
    notify(await invoke("copy_shared_to_isolated", { version: v }));
    sizes.value = { ...sizes.value, [v]: undefined };
  } catch (e) { notify("" + e); }
}

async function clearIsolated(v) {
  closeMenu();
  if (!(await ask("将删除该版本的隔离数据（保留版本本身）。\n此操作不可恢复，继续吗？", { title: "清理隔离数据", kind: "warning" }))) return;
  try {
    notify(await invoke("clear_isolated_data", { version: v }));
    sizes.value = { ...sizes.value, [v]: 0 };
  } catch (e) { notify("" + e); }
}

async function applyRoot() {
  try {
    notify(await invoke("set_root", { root: rootInput.value.trim() || null }));
    await refresh();
  } catch (e) { notify("" + e); }
}

async function resetRoot() {
  rootInput.value = "";
  await applyRoot();
}

async function openUrl(url) {
  try { await invoke("open_url", { url }); }
  catch (e) { notify("打开链接失败：" + e); }
}

async function openDir(which) {
  try { await invoke("open_dir", { which }); }
  catch (e) { notify("" + e); }
}

/// 维护：手动触发
const maintBusy = ref("");
async function doMaintenance(kind) {
  maintBusy.value = kind;
  try {
    const msg = await invoke("run_maintenance", { kind });
    notify(msg);
    await refresh();
  } catch (e) {
    notify("" + e);
  } finally {
    maintBusy.value = "";
  }
}

/// 维护：切换自动维护开关
async function toggleAutoMaint(on) {
  try {
    await invoke("set_auto_maintenance", { enabled: on });
    await refresh();
  } catch (e) {
    notify("" + e);
  }
}

/// unix 秒 -> 可读时间
function fmtTs(sec) {
  if (!sec) return "从未";
  const d = new Date(sec * 1000);
  const p = (n) => String(n).padStart(2, "0");
  return d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()) + " " + p(d.getHours()) + ":" + p(d.getMinutes());
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
  unlistenProgress = await listen("install-progress", (e) => {
    const p = e.payload;
    installStage.value = p.stage || "";
    installStep.value = p.step || 0;
    installTotal.value = p.total || 0;
    installDetail.value = p.detail || "";
    installFraction.value = p.fraction || 0;
  });
  await runEnvCheck();
  document.addEventListener("click", closeMenu);
});

onUnmounted(() => {
  window.removeEventListener("error", onWindowError);
  window.removeEventListener("unhandledrejection", onUnhandledRejection);
  document.removeEventListener("click", closeMenu);
  if (unlistenProgress) unlistenProgress();
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
    <section class="panel">
      <div class="panel-head">
        <h2>
          环境检查
          <span class="badge-ok" v-if="envChecked && envPassed">通过</span>
          <span class="badge-err" v-else-if="envChecked">未通过</span>
        </h2>
        <button class="btn small" @click="runEnvCheck" :disabled="loading">重新检查</button>
      </div>
      <ul class="env-list" v-if="envChecks.length">
        <li
          v-for="c in envChecks"
          :key="c.name"
          class="env-item"
          :class="{ 'env-item-fail': !c.ok }"
        >
          <div class="env-row">
            <span class="env-icon" :class="c.ok ? 'ok' : 'err'">{{ c.ok ? "✓" : "✕" }}</span>
            <span class="env-name">{{ c.name }}</span>
            <span class="env-detail">{{ c.detail }}</span>
            <span class="env-critical" v-if="!c.ok && !c.critical">（非致命）</span>
          </div>
          <!-- 未通过：显示安装引导 -->
          <div class="env-guide" v-if="!c.ok && c.install_hint">
            <span class="env-guide-text">{{ c.install_hint }}</span>
            <button
              v-if="c.install_url"
              class="btn small primary"
              @click="openUrl(c.install_url)"
            >打开下载页</button>
          </div>
        </li>
      </ul>
      <div v-else class="hint">正在检查环境...</div>
    </section>

    <section class="panel">
      <div class="panel-head">
        <h2>已安装版本</h2>
        <span class="count" v-if="installed.length">{{ installed.length }}</span>
      </div>

      <div v-if="!installed.length" class="empty">
        还没有安装任何版本，从下方列表安装一个吧
      </div>

      <ul class="ver-list" v-else>
        <li v-for="v in installed" :key="v.version" class="ver-item" :class="{ 'item-selected': isSelected(v.version) }">
          <input type="checkbox" class="sel-box" :checked="isSelected(v.version)" :disabled="batchBusy" @change="toggleSelect(v.version)" />
          <div class="ver-main">
            <span class="ver-num ver-num-clickable" :title="'点击查看该版本更新说明'" @click="toggleInstalledNote(v.version)">
              {{ v.version }}<span class="ver-caret">{{ installedNoteVer === v.version ? "▾" : "▸" }}</span>
            </span>
            <span class="badge" v-if="v.is_default">默认</span>
            <span class="badge badge-iso" v-if="v.isolated">隔离</span>
            <span class="badge badge-shared" v-else title="使用共享 home：多版本混用可能导致插件/依赖版本错配，测试版建议开隔离">共享 home</span>
            <span class="ver-meta" v-if="v.installed_at">安装于 {{ v.installed_at }}</span>
            <span class="ver-meta" v-if="verSizes[v.version] != null">
              占用 {{ fmtSize(verSizes[v.version].total) }}
              <template v-if="verSizes[v.version].shared_count">
                （复用 {{ fmtSize(verSizes[v.version].shared_size) }} / 独占 {{ fmtSize(verSizes[v.version].exclusive_size) }}）
              </template>
            </span>
          </div>
          <div class="ver-actions">
            <button class="btn primary" @click="run(v.version)" :disabled="!!runningVersion || isBlocked(v.version)">{{ runningVersion === v.version ? "启动中..." : "运行" }}</button>
            <button class="btn" @click="openInBrowser(v.version)" :disabled="isBlocked(v.version)" title="在新终端启动并在系统浏览器打开">浏览器打开</button>
            <button class="btn" @click="setDefault(v.version)" :disabled="v.is_default || isBlocked(v.version)">设为默认</button>
            <button class="btn" @click="createShortcut(v.version)" :disabled="isBlocked(v.version)" title="在桌面创建 DSH 快捷方式">桌面快捷方式</button>
            <button class="btn" @click="scanVersionSize(v.version)" :disabled="scanningVer === v.version || isBlocked(v.version)" title="统计该版本占用的磁盘空间">
              {{ scanningVer === v.version ? "扫描中..." : (verSizes[v.version] != null ? "重新扫描占用" : "扫描占用") }}
            </button>
            <div class="menu-wrap" v-if="v.isolated">
              <button class="btn active" @click.stop="toggleMenu(v.version)" :disabled="isBlocked(v.version)">
                已隔离 ▾
              </button>
              <div class="menu" v-if="openMenu === v.version" @click.stop>
                <button class="menu-item" @click="toggleIsolated(v.version, true)">关闭隔离</button>
                <button class="menu-item" @click="openIsolatedDir(v.version)">打开隔离目录</button>
                <button class="menu-item" @click="scanSize(v.version)" :disabled="scanning === v.version">
                  <span v-if="scanning === v.version">扫描中...</span>
                  <span v-else-if="sizes[v.version] != null">占用 {{ fmtSize(sizes[v.version]) }}（重新扫描）</span>
                  <span v-else>扫描占用大小</span>
                </button>
                <button class="menu-item" @click="copyShared(v.version)">复制共享数据到此</button>
                <button class="menu-item danger" @click="clearIsolated(v.version)">清理隔离数据</button>
              </div>
            </div>
            <button
              v-else
              class="btn"
              @click="toggleIsolated(v.version, false)"
              :disabled="isBlocked(v.version)"
              title="开启隔离（该版本使用独立数据目录）"
            >隔离</button>
            <button class="btn danger" @click="uninstall(v.version)" :disabled="!!uninstalling || batchBusy">{{ uninstalling === v.version ? "卸载中..." : "卸载" }}</button>
          </div>
          <div class="notes-panel notes-inline" v-if="installedNoteVer === v.version">
            <div class="notes-head" @click="installedNoteVer = null" title="点击收起">
              <span class="notes-title">DSH {{ v.version }} 更新说明</span>
              <span class="notes-close">×</span>
            </div>
            <div class="notes-body">
              <div v-if="!notesCache[v.version] || notesCache[v.version].status === 'loading'" class="notes-hint">加载中...</div>
              <div v-else-if="notesCache[v.version].status === 'ok' && notesCache[v.version].empty" class="notes-hint">该版本没有提供更新说明。</div>
              <div v-else-if="notesCache[v.version].status === 'ok'" class="notes-html" v-html="notesCache[v.version].html"></div>
              <div v-else class="notes-hint">无法获取更新说明（可能网络不通）。可点击下方按钮在浏览器查看。</div>
            </div>
            <div class="notes-foot">
              <button class="btn small" @click="openUrl((notesCache[v.version] && notesCache[v.version].url) || releaseUrl(v.version))">在浏览器打开原文</button>
            </div>
          </div>
        </li>
      </ul>

      <!-- 批量操作栏 -->
      <div class="batch-bar" v-if="installed.length">
        <label class="batch-select-all">
          <input type="checkbox"
            :checked="installed.length > 0 && installed.every((x) => isSelected(x.version))"
            :disabled="batchBusy"
            @change="toggleSelectAll($event.target.checked)" />
          全选
        </label>
        <template v-if="selected.size">
          <span class="batch-count">已选 {{ selected.size }} 项</span>
          <button class="btn small" @click="clearSelection" :disabled="batchBusy">清除选择</button>
          <button class="btn danger small" @click="batchUninstall" :disabled="batchBusy">
            {{ batchBusy ? "批量卸载中..." : "批量卸载" }}
          </button>
        </template>
      </div>
    </section>

    <section class="panel">
      <div class="panel-head">
        <h2>可用版本</h2>
        <button class="btn small" @click="loadRemote" :disabled="loading">
          {{ loading ? "加载中..." : "刷新列表" }}
        </button>
      </div>

      <div class="install-row">
        <input v-model="installInput" placeholder="输入版本号，如 0.1.5" @keyup.enter="install()" />
        <button class="btn primary" @click="install()" :disabled="loading">安装</button>
      </div>

      <div class="notes-panel" v-if="remoteNoteVer">
        <div class="notes-head" @click="remoteNoteVer = null" title="点击收起">
          <span class="notes-title">DSH {{ remoteNoteVer }} 更新说明</span>
          <span class="notes-close">×</span>
        </div>
        <div class="notes-body">
          <div v-if="!notesCache[remoteNoteVer]" class="notes-hint">加载中...</div>
          <div v-else-if="notesCache[remoteNoteVer].status === 'loading'" class="notes-hint">加载中...</div>
          <div v-else-if="notesCache[remoteNoteVer].status === 'ok' && notesCache[remoteNoteVer].empty" class="notes-hint">该版本没有提供更新说明。</div>
          <div v-else-if="notesCache[remoteNoteVer].status === 'ok'" class="notes-html" v-html="notesCache[remoteNoteVer].html"></div>
          <div v-else class="notes-hint">无法获取更新说明（可能网络不通）。可点击下方按钮在浏览器查看。</div>
        </div>
        <div class="notes-foot">
          <button class="btn small" @click="openUrl(notesCache[remoteNoteVer] && notesCache[remoteNoteVer].url || releaseUrl(remoteNoteVer))">在浏览器打开原文</button>
        </div>
      </div>

      <div class="install-progress" v-if="installStage">
        <div class="spinner"></div>
        <div class="prog-body">
          <div class="prog-line">
            <span class="stage-text">{{ installStage }}</span>
            <span class="prog-detail" v-if="installDetail">{{ installDetail }}</span>
            <span class="prog-count" v-if="installTotal">{{ installStep }}/{{ installTotal }}</span>
          </div>
          <div class="prog-bar"><div class="prog-fill" :style="{ width: progressPct() + '%' }"></div></div>
        </div>
      </div>

      <div class="remote-toolbar" v-if="remote.length">
        <input v-model="remoteQuery" class="input remote-search" placeholder="搜索版本号，如 0.1.7" />
        <button class="btn small" @click="remoteSortDesc = !remoteSortDesc" :title="remoteSortDesc ? '当前最新在前' : '当前最早在前'">
          {{ remoteSortDesc ? "最新在前 ↓" : "最早在前 ↑" }}
        </button>
      </div>
      <div class="chips" v-if="remote.length">
        <button
          v-for="v in filteredRemote"
          :key="v"
          class="chip"
          :class="{ 'chip-broken': isBroken(v) }"
          :title="isBroken(v) ? '此版本上次安装失败（依赖已下架），点右下角可清除标记' : ''"
          @click="pickVersion(v)"
          :disabled="loading"
        >{{ v }}</button>
        <div class="hint" v-if="!filteredRemote.length">没有匹配的版本</div>
      </div>
      <div v-else class="hint">点击“刷新列表”从 npm 拉取所有可安装版本</div>
      <div class="broken-hint" v-if="state && state.broken_versions && state.broken_versions.length">
        灰色版本曾被标记为无法安装（依赖已下架）：{{ state.broken_versions.join("、") }}
        <button class="btn small" @click="clearBroken">清除标记</button>
      </div>
    </section>

    <section class="panel">
      <div class="panel-head">
        <h2>路径设置</h2>
        <button class="btn small" @click="clearDefault" v-if="state && state.default_version">清除默认</button>
      </div>

      <div class="field">
        <label>数据根目录</label>
        <div class="field-row">
          <input v-model="rootInput" placeholder="留空则使用软件所在目录" />
          <button class="btn primary" @click="applyRoot">应用</button>
          <button class="btn" @click="resetRoot">默认</button>
        </div>
      </div>

      <label class="checkbox-row">
        <input
          type="checkbox"
          :checked="warnSharedHomeEnabled"
          @change="setWarnSharedHome($event.target.checked)"
        />
        <span>运行「共享 home」版本时提示（多版本混用可能错配）</span>
      </label>

      <div class="paths" v-if="state">
        <div class="path-item" @click="openDir('versions')">
          <span class="path-name">versions</span>
          <span class="path-desc">已安装的各版本 dsh</span>
          <code>{{ state.versions_dir }}</code>
        </div>
        <div class="path-item" @click="openDir('home')">
          <span class="path-name">home</span>
          <span class="path-desc">dsh 数据目录（相当于 ~/.dsh）</span>
          <code>{{ state.home_dir }}</code>
        </div>
        <div class="path-item" @click="openDir('store')">
          <span class="path-name">store</span>
          <span class="path-desc">pnpm 依赖仓库（硬链接省磁盘）</span>
          <code>{{ state.store_dir }}</code>
        </div>
        <div class="path-item" @click="openDir('cache')">
          <span class="path-name">cache</span>
          <span class="path-desc">pnpm 下载/元数据缓存（可安全删除）</span>
          <code>{{ state.cache_dir }}</code>
        </div>
        <div class="path-item" @click="openDir('state')">
          <span class="path-name">state</span>
          <span class="path-desc">pnpm 运行状态（可安全删除）</span>
          <code>{{ state.state_dir }}</code>
        </div>
      </div>
      <div class="hint">点击任意路径可在资源管理器中打开</div>

      <!-- 维护 -->
      <div class="maint" v-if="state">
        <label class="checkbox-row">
          <input type="checkbox" :checked="state.maintenance.auto_enabled" @change="toggleAutoMaint($event.target.checked)" />
          <span>启动时自动维护</span>
        </label>
        <div class="maint-rules">
          <div>· 每次启动清理<strong>孤立缓存</strong>（版本已卸载的 WebView2 残留）</div>
          <div>· 距上次 ≥ 7 天时，自动<strong>回收依赖仓库</strong>（pnpm store prune）</div>
        </div>
        <div class="maint-status">
          <span>上次清理孤立缓存：{{ fmtTs(state.maintenance.last_cleanup_at) }}（{{ state.maintenance.last_cleanup_count }} 项）</span>
          <span>上次回收依赖仓库：{{ fmtTs(state.maintenance.last_prune_at) }}</span>
        </div>
        <div class="maint-actions">
          <button class="btn small" @click="doMaintenance('cleanup')" :disabled="!!maintBusy">{{ maintBusy === 'cleanup' ? '清理中...' : '清理孤立缓存' }}</button>
          <button class="btn small" @click="doMaintenance('prune')" :disabled="!!maintBusy">{{ maintBusy === 'prune' ? '回收中...' : '回收依赖仓库' }}</button>
          <button class="btn small" @click="openDir('logs')">打开日志目录</button>
        </div>
      </div>
    </section>
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

  <!-- 安装失败弹窗（按错误类型分类） -->
  <div class="modal-mask" v-if="retryOpen" @click.self="retryOpen = false">
    <div class="modal">
      <!-- 网络类 -->
      <template v-if="retryKind === 'network'">
        <h3>安装失败（网络问题）</h3>
        <div class="modal-desc">
          安装 <b>{{ retryVersion }}</b> 时网络请求失败。可换个 npm 源重试：
        </div>
        <div class="field">
          <label>选择源</label>
          <select v-model="retryRegistry" class="input">
            <option v-for="r in REGISTRIES" :key="r.value" :value="r.value">{{ r.label }}</option>
          </select>
        </div>
        <div class="field">
          <label>或自定义源（填写后优先）</label>
          <input v-model="retryCustom" class="input" placeholder="https://.../npm/" />
        </div>
      </template>

      <!-- 私有包 / 版本不完整 -->
      <template v-else>
        <h3>此版本无法安装</h3>
        <div class="modal-desc">
          安装 <b>{{ retryVersion }}</b> 失败：该版本依赖的官方子包已下架（或未完整发布），
          任何 npm 源都无法获取。<b>建议换用更新的版本</b>。
        </div>
        <details class="retry-detail">
          <summary>其他安装方式（进阶）</summary>
          <div class="adv-body">
            <p><b>1. 配置官方私有源令牌</b>（需官方授权）：</p>
            <pre>//registry.npmjs.org/:_authToken=&lt;你的 NPM_TOKEN&gt;
@deepseek-ai:registry=https://registry.npmjs.org/</pre>
            <p>将上面两行写入 <code>%USERPROFILE%\.npmrc</code>，再回本工具重试。</p>
            <p><b>2. 从源码构建</b>：</p>
            <pre>git clone https://github.com/deepseek-ai/deepseek-harness.git
cd deepseek-harness
pnpm install &amp;&amp; pnpm build</pre>
            <p>源码构建不受 npm 包发布状态影响，但需自行维护版本。</p>
          </div>
        </details>
      </template>

      <details class="retry-detail">
        <summary>原始错误</summary>
        <pre>{{ retryError }}</pre>
      </details>
      <div class="modal-actions">
        <button class="btn" @click="retryOpen = false">{{ retryKind === 'network' ? '取消' : '知道了' }}</button>
        <button v-if="retryKind === 'network'" class="btn primary" @click="retryInstall">换源重试</button>
        <button v-else class="btn primary" @click="retryOpen = false">好</button>
      </div>
    </div>
  </div>
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

.env-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.env-item {
  display: flex; flex-direction: column; align-items: stretch; gap: 0;
  padding: 8px 12px; border-radius: 8px; background: #fafbfc;
  font-size: 13px;
}
.env-icon {
  width: 18px; height: 18px; border-radius: 50%; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center;
  font-size: 11px; font-weight: 700; color: #fff;
}
.env-icon.ok { background: #30a46c; }
.env-icon.err { background: #e5484d; }
.env-name { font-weight: 600; color: #374151; min-width: 90px; }
.env-detail { color: #6b7280; font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.env-critical { color: #e5484d; font-size: 11px; }
.env-item-fail { background: #fff8f8; }
.env-row { display: flex; align-items: center; gap: 10px; }
.env-guide {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  margin-top: 8px; margin-left: 28px;
  padding: 8px 10px; background: #fff; border: 1px solid #f0d0d0;
  border-radius: 6px; font-size: 12px; color: #6b7280; line-height: 1.5;
}
.env-guide-text { flex: 1; min-width: 200px; }
.badge-ok { background: #e8f5ec; color: #2f9e5f; font-size: 11px; padding: 2px 8px; border-radius: 10px; margin-left: 6px; }
.badge-err { background: #fdf2f2; color: #d9534f; font-size: 11px; padding: 2px 8px; border-radius: 10px; margin-left: 6px; }

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

/* 版本更新说明面板（可用版本：输入框下方；已安装：行内展开） */
.notes-panel {
  margin: 0 0 12px; padding: 10px 14px; border: 1px solid #e2e5ea; border-radius: 9px;
  background: #fafbfc;
}
.notes-inline { flex-basis: 100%; margin: 8px 0 0; }
/* 标题栏整行可点收起：加 padding 撑满、hover 有反馈，鼠标不必对准 × */
.notes-head {
  display: flex; align-items: center; justify-content: space-between;
  margin: -6px -8px 4px; padding: 6px 8px; border-radius: 6px;
  cursor: pointer; user-select: none; transition: background .12s;
}
.notes-head:hover { background: #eef1f5; }
.notes-title { font-weight: 600; font-size: 13px; color: #374151; }
.notes-close { font-size: 18px; line-height: 1; color: #9aa1ab; flex-shrink: 0; }
.notes-head:hover .notes-close { color: #d9534f; }
.notes-body { max-height: 260px; overflow-y: auto; }
.notes-hint { color: #9aa1ab; font-size: 12px; }
.notes-html { font-size: 13px; color: #374151; line-height: 1.65; }
.notes-html h1, .notes-html h2, .notes-html h3, .notes-html h4 {
  font-size: 13px; font-weight: 600; margin: 10px 0 4px; color: #1f2937;
}
.notes-html h3:first-child, .notes-html h4:first-child { margin-top: 0; }
.notes-html ul, .notes-html ol { margin: 4px 0; padding-left: 20px; }
.notes-html li { margin: 2px 0; }
.notes-html p { margin: 6px 0; }
.notes-html a { color: #4f6ef7; }
.notes-html code { background: #eef1f5; padding: 1px 5px; border-radius: 4px; font-size: 12px; }
.notes-html img { max-width: 100%; }
.notes-foot { margin-top: 8px; display: flex; justify-content: flex-end; }
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

.menu-wrap { position: relative; }
.menu {
  position: absolute; top: calc(100% + 4px); right: 0; z-index: 50;
  background: #fff; border: 1px solid #e2e5ea; border-radius: 8px;
  box-shadow: 0 6px 20px rgba(0,0,0,0.12); min-width: 200px;
  padding: 4px; display: flex; flex-direction: column;
}
.menu-item {
  text-align: left; border: none; background: none; color: #374151;
  padding: 8px 12px; border-radius: 6px; font-size: 13px; cursor: pointer;
  font-family: inherit; transition: background .12s;
}
.menu-item:hover:not(:disabled) { background: #f2f4f8; }
.menu-item:disabled { opacity: .6; cursor: not-allowed; }
.menu-item.danger { color: #d9534f; }
.menu-item.danger:hover { background: #fdf2f2; }

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

.install-row { display: flex; gap: 8px; margin-bottom: 12px; }
.install-row input {
  flex: 1; padding: 8px 12px; border: 1px solid #dcdfe4;
  border-radius: 7px; font-size: 13px; font-family: inherit; outline: none;
}
.install-row input:focus { border-color: #4f6ef7; }

.install-progress {
  display: flex; align-items: flex-start; gap: 10px;
  padding: 10px 14px; margin-bottom: 12px;
  background: #f5f7ff; border: 1px solid #e2e7ff; border-radius: 8px;
}
.prog-body { flex: 1; min-width: 0; }
.prog-line { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
.prog-detail { font-size: 12px; color: #6b7280; }
.prog-count { font-size: 12px; color: #9aa1ab; margin-left: auto; font-variant-numeric: tabular-nums; }
.prog-bar { height: 6px; background: #e2e7ff; border-radius: 3px; overflow: hidden; }
.prog-fill { height: 100%; background: #4f6ef7; border-radius: 3px; transition: width .3s ease; }
.spinner {
  width: 15px; height: 15px; flex-shrink: 0;
  border: 2px solid #c9d4ff; border-top-color: #4f6ef7;
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
.stage-text { font-size: 13px; color: #4f6ef7; }

.remote-toolbar { display: flex; gap: 8px; margin-bottom: 10px; }
.remote-search { flex: 1; min-width: 0; padding: 6px 12px; border: 1px solid #dcdfe4; border-radius: 7px; font-size: 13px; font-family: inherit; outline: none; }
.remote-search:focus { border-color: #4f6ef7; }
.chips { display: flex; flex-wrap: wrap; gap: 6px; max-height: 120px; overflow-y: auto; }
.chip {
  border: 1px solid #e2e5ea; background: #fafbfc; color: #4b5563;
  padding: 4px 10px; border-radius: 6px; font-size: 12px;
  cursor: pointer; transition: all .12s; font-family: inherit;
  font-variant-numeric: tabular-nums;
}
.chip:hover:not(:disabled) { border-color: #4f6ef7; color: #4f6ef7; background: #f5f7ff; }
.chip:disabled { opacity: .5; cursor: not-allowed; }

.checkbox-row {
  display: flex; align-items: center; gap: 8px;
  font-size: 12px; color: #6b7280; margin: 4px 0 14px; cursor: pointer;
}
.checkbox-row input { cursor: pointer; }

/* ---------- 维护 ---------- */
.maint { margin-top: 16px; padding-top: 14px; border-top: 1px solid #eef1f5; }
.maint-rules { font-size: 12px; color: #6b7280; line-height: 1.8; margin: 4px 0 10px; }
.maint-status { font-size: 12px; color: #9aa1ab; line-height: 1.8; margin-bottom: 10px; }
.maint-status span { display: block; }
.maint-actions { display: flex; gap: 8px; flex-wrap: wrap; }
.maint-actions .btn { color: #4f6ef7; border-color: #c3cdf5; }
.maint-actions .btn:hover:not(:disabled) { background: #f5f7ff; border-color: #4f6ef7; }
.field { margin-bottom: 14px; }
.field label { display: block; font-size: 12px; color: #6b7280; margin-bottom: 6px; }
.field-row { display: flex; gap: 8px; }
.field-row input {
  flex: 1; padding: 8px 12px; border: 1px solid #dcdfe4;
  border-radius: 7px; font-size: 13px; font-family: inherit; outline: none;
}
.field-row input:focus { border-color: #4f6ef7; }

.paths { display: flex; flex-direction: column; gap: 6px; }
.path-item {
  display: flex; align-items: center; gap: 10px;
  padding: 8px 12px; border-radius: 8px; cursor: pointer;
  transition: background .15s;
}
.path-item:hover { background: #f5f7ff; }
.path-name {
  font-size: 12px; font-weight: 600; color: #4f6ef7;
  min-width: 64px; font-family: ui-monospace, Consolas, monospace;
}
.path-desc {
  font-size: 11px; color: #9aa1ab; min-width: 180px;
}
.path-item code {
  font-size: 12px; color: #6b7280; overflow: hidden;
  text-overflow: ellipsis; white-space: nowrap;
}

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

/* 换源重试弹窗 */
.modal-mask {
  position: fixed; inset: 0; z-index: 1000;
  background: rgba(20,26,36,.45);
  display: flex; align-items: center; justify-content: center;
}
.modal {
  width: 480px; max-width: 92vw; max-height: 86vh; overflow: auto;
  background: #fff; border-radius: 10px; padding: 18px 20px;
  box-shadow: 0 16px 48px rgba(10,20,40,.28);
}
.modal h3 { margin: 0 0 10px; font-size: 15px; }
.modal-desc { font-size: 13px; color: #4b5563; margin-bottom: 14px; line-height: 1.5; }
.field { margin-bottom: 12px; }
.field label { display: block; font-size: 12px; color: #6b7280; margin-bottom: 6px; }
.input {
  width: 100%; box-sizing: border-box;
  padding: 8px 12px; border: 1px solid #dcdfe4;
  border-radius: 7px; font-size: 13px; font-family: inherit; outline: none;
  background: #fff;
}
.input:focus { border-color: #4f6ef7; }
.retry-detail { margin-bottom: 12px; font-size: 12px; color: #6b7280; }
.retry-detail summary { cursor: pointer; }
.adv-body { margin-top: 6px; line-height: 1.6; }
.adv-body p { margin: 8px 0 4px; }
.adv-body pre {
  margin: 4px 0; padding: 8px; background: #f7f8fa; border-radius: 6px;
  font-size: 11px; white-space: pre-wrap; word-break: break-all;
}
.adv-body code { background: #eef1f5; padding: 1px 4px; border-radius: 3px; }
.chip-broken {
  opacity: .5; text-decoration: line-through;
  border-style: dashed; cursor: not-allowed;
}
.broken-hint {
  margin-top: 10px; font-size: 12px; color: #9aa1ab;
  display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
}
.retry-detail pre {
  margin: 6px 0 0; padding: 8px; max-height: 160px; overflow: auto;
  background: #f7f8fa; border-radius: 6px; font-size: 11px;
  white-space: pre-wrap; word-break: break-all;
}
.modal-actions { display: flex; justify-content: flex-end; gap: 8px; }

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
