<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";

const state = ref(null);
const installed = ref([]);
const remote = ref([]);
const loading = ref(false);
const toast = ref("");
const toastExpanded = ref(false);
const installInput = ref("");
const rootInput = ref("");
const installStage = ref("");
const installStep = ref(0);
const installTotal = ref(0);
const installDetail = ref("");
const installFraction = ref(0);
const uninstalling = ref(""); // 正在卸载的版本号
const runningVersion = ref(""); // 正在启动的版本号（用于禁用按钮 + 显示反馈）
const envChecks = ref([]);
const envChecked = ref(false);
const envPassed = ref(false);
let toastTimer = null;
let unlistenProgress = null;
// 「运行共享 home 版本时提示」开关（持久化到 localStorage）
const warnSharedHomeEnabled = ref(localStorage.getItem("dsh-multiver.warnSharedHome") !== "0");

// ---- 整合包导入相关状态 ----
const dragging = ref(false);
const importPreview = ref(null);   // 预览结果（确认框用）
const importPath = ref("");        // 待导入的文件路径
const importBusy = ref(false);
const importStage = ref("");
let unlistenCliImport = null;
let unlistenDragDrop = null;

// 列表分组
const versionList = computed(() => installed.value.filter((v) => v.kind !== "modpack"));
const modpackList = computed(() => installed.value.filter((v) => v.kind === "modpack"));

// ---- 市场 ----
const marketPacks = ref([]);       // 索引里的整合包
const marketLoading = ref(false);
const marketLoaded = ref(false);
const marketQuery = ref("");       // 搜索词
const marketCategory = ref("");    // 分类过滤（"" = 全部）
const marketInstalling = ref("");  // 正在安装的包 id
// 市场面板折叠状态（默认折叠，持久化）
const marketCollapsed = ref(localStorage.getItem("dsh-multiver.marketCollapsed") !== "0");
function toggleMarket() {
  marketCollapsed.value = !marketCollapsed.value;
  localStorage.setItem("dsh-multiver.marketCollapsed", marketCollapsed.value ? "1" : "0");
  // 展开时首次加载（避免默认折叠也联网）
  if (!marketCollapsed.value) loadMarket();
}

/// 已安装的 modpack（name@version 集合），用于市场「已安装」标记
const installedKeys = computed(() => {
  const s = new Set();
  for (const v of modpackList.value) {
    if (v.modpack_name && v.modpack_version) {
      s.add(v.modpack_name + "@" + v.modpack_version);
    }
  }
  return s;
});

/// 市场分类列表（去重、排序；uncategorized 排最后）
const marketCategories = computed(() => {
  const s = new Set();
  for (const p of marketPacks.value) s.add(p.category || "uncategorized");
  const arr = [...s].filter((c) => c !== "uncategorized").sort();
  if (s.has("uncategorized")) arr.push("uncategorized");
  return arr;
});

/// 过滤后的市场列表
const filteredMarket = computed(() => {
  const q = marketQuery.value.trim().toLowerCase();
  return marketPacks.value.filter((p) => {
    if (marketCategory.value && (p.category || "uncategorized") !== marketCategory.value) {
      return false;
    }
    if (!q) return true;
    const hay = [
      p.name,
      p.displayName ? (typeof p.displayName === "string" ? p.displayName : Object.values(p.displayName).join(" ")) : "",
      p.description ? (typeof p.description === "string" ? p.description : Object.values(p.description).join(" ")) : "",
      p.author || "",
      p.owner || "",
    ].join(" ").toLowerCase();
    return hay.includes(q);
  });
});

function packDisplayName(p) {
  const d = p.displayName;
  if (!d) return p.name;
  if (typeof d === "string") return d;
  return d["zh-CN"] || d["zh"] || d["en-US"] || d["en"] || Object.values(d)[0] || p.name;
}

function packDescription(p) {
  const d = p.description;
  if (!d) return "";
  if (typeof d === "string") return d;
  return d["zh-CN"] || d["zh"] || d["en-US"] || d["en"] || Object.values(d)[0] || "";
}

/// 该包是否已安装（按 name@version）
function isPackInstalled(p) {
  return installedKeys.value.has(p.name + "@" + p.version);
}

async function loadMarket(force) {
  if (marketLoading.value) return;
  if (marketLoaded.value && !force) return;
  marketLoading.value = true;
  try {
    marketPacks.value = await invoke("market_list");
    marketLoaded.value = true;
  } catch (e) {
    notify("获取市场列表失败：" + e);
  } finally {
    marketLoading.value = false;
  }
}

async function installFromMarket(p) {
  if (marketInstalling.value) return;
  const label = packDisplayName(p);
  if (!(await ask("安装整合包「" + label + " " + p.version + "」？", { title: "市场安装", kind: "info" }))) return;
  marketInstalling.value = p.id;
  installStage.value = "正在准备...";
  try {
    const name = await invoke("market_install", { pack: p });
    await refresh();
    notify("已安装：" + name);
    const open = await ask("整合包「" + label + "」安装完成，是否立即打开？", { title: "安装完成", kind: "info" });
    if (open) await run(name);
  } catch (e) {
    notify(String(e));
  } finally {
    marketInstalling.value = "";
    installStage.value = "";
  }
}

/// 导出整合包实例为 .dspack。
async function exportModpack(instance) {
  try {
    const target = await save({
      title: "导出整合包",
      defaultPath: instance + ".dspack",
      filters: [{ name: "DSH 整合包", extensions: ["dspack"] }],
    });
    if (!target) return;
    installStage.value = "正在导出...";
    const msg = await invoke("export_modpack", { instance, output: target });
    notify(msg);
  } catch (e) {
    notify(String(e));
  } finally {
    installStage.value = "";
  }
}
function fmtBytes(n) {
  if (n == null) return "";
  if (n < 1024) return n + " B";
  if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB";
  return (n / 1024 / 1024).toFixed(1) + " MB";
}

/// 悬停浮层：当前展开的实例
const hoverItem = ref(null);

function fmtMultiLang(v) {
  return v || "";
}

/// 触发文件选择 → 预览
async function pickAndPreview() {
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: "DSH 整合包", extensions: ["dspack"] }],
    });
    if (!selected) return;
    await previewModpack(selected);
  } catch (e) {
    notify("选择文件失败：" + e);
  }
}

/// 预览并弹确认
async function previewModpack(path) {
  if (importBusy.value) return;
  importBusy.value = true;
  importStage.value = "正在解析整合包...";
  try {
    const p = await invoke("preview_dspack", { path });
    importPreview.value = p;
    importPath.value = path;
  } catch (e) {
    notify(String(e));
  } finally {
    importBusy.value = false;
    importStage.value = "";
  }
}

/// 用户确认导入
async function confirmImport(mode) {
  // mode: "normal" | "replace" | "keep"
  const p = importPreview.value;
  if (!p) return;
  importPreview.value = null;
  importBusy.value = true;
  importStage.value = "正在导入...";
  try {
    let suffix = null;
    let replace = false;
    if (mode === "keep") {
      suffix = timestampSuffix();
    } else if (mode === "replace") {
      replace = true;
    }
    const name = await invoke("import_dspack", {
      path: importPath.value,
      suffix,
      replace,
    });
    await refresh();
    const open = await ask(
      "整合包 " + p.display_name + " 导入完成，是否立即打开？",
      { title: "导入完成", kind: "info" }
    );
    if (open) await run(name);
  } catch (e) {
    notify(String(e));
  } finally {
    importBusy.value = false;
    importStage.value = "";
  }
}

function cancelImport() {
  importPreview.value = null;
  importPath.value = "";
}

function timestampSuffix() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return (
    d.getFullYear() +
    p(d.getMonth() + 1) +
    p(d.getDate()) +
    "-" +
    p(d.getHours()) +
    p(d.getMinutes()) +
    p(d.getSeconds())
  );
}

/// 处理拖入的文件路径（来自 Tauri 原生拖放事件）。
function handleDroppedPaths(paths) {
  dragging.value = false;
  if (!paths || !paths.length) return;
  if (paths.length > 1) {
    notify("一次只能导入一个 .dspack 文件");
    return;
  }
  const path = paths[0];
  const name = path.split(/[\\/]/).pop() || "";
  if (!name.toLowerCase().endsWith(".dspack")) {
    notify("只支持 .dspack 整合包文件");
    return;
  }
  previewModpack(path);
}

/// 是否整合包实例
function isModpack(v) {
  return v && v.kind === "modpack";
}

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

/// 各阶段起始百分比与权重（按耗时分配，和 ≈ 100）。索引 = step - 1。
const STAGE_START = [0, 1, 2, 4, 10, 35, 90, 97];
const STAGE_WEIGHT = [1, 1, 2, 6, 25, 55, 7, 3];

/// 版本安装进度百分比（0-100）。
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

  loading.value = true;
  installStage.value = "准备中...";
  try {
    const msg = await invoke("install_version", { version: ver });
    notify(msg);
    await refresh();
  } catch (e) {
    notify("" + e);
  } finally {
    loading.value = false;
    installStage.value = "";
  }
}

async function uninstall(v) {
  if (uninstalling.value) return;
  if (!(await ask("确定卸载 " + v + " ?", { title: "卸载", kind: "warning" }))) return;
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
const verSizes = ref({});         // 版本号 -> 整版本字节数

async function scanVersionSize(v) {
  scanningVer.value = v;
  try {
    const bytes = await invoke("scan_version_size", { version: v });
    verSizes.value = { ...verSizes.value, [v]: bytes };
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

async function openDir(which) {
  try { await invoke("open_dir", { which }); }
  catch (e) { notify("" + e); }
}

onMounted(async () => {
  await refresh();
  // 若市场面板上次是展开的，启动即加载
  if (!marketCollapsed.value) loadMarket();
  unlistenProgress = await listen("install-progress", (e) => {
    const p = e.payload;
    if (typeof p === "string") {
      // 整合包导入：阶段文字（字符串）
      importStage.value = p;
    } else if (p && typeof p === "object") {
      // 版本安装：结构化进度（对象）
      installStage.value = p.stage || "";
      installStep.value = p.step || 0;
      installTotal.value = p.total || 0;
      installDetail.value = p.detail || "";
      installFraction.value = p.fraction || 0;
    }
  });
  // 命令行 --import：界面就绪后自动弹出导入确认
  unlistenCliImport = await listen("cli-import", (e) => {
    previewModpack(e.payload);
  });
  // 原生拖放：Tauri 默认在 OS 层拦截拖放，HTML5 的 drop 事件收不到，
  // 必须用 webview 的 onDragDropEvent。
  try {
    const webview = getCurrentWebview();
    unlistenDragDrop = await webview.onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "over") {
        dragging.value = true;
      } else if (p.type === "drop") {
        handleDroppedPaths(p.paths);
      } else {
        // "leave"
        dragging.value = false;
      }
    });
  } catch (e) {
    console.warn("注册拖放监听失败", e);
  }
  await runEnvCheck();
  document.addEventListener("click", closeMenu);
});

onUnmounted(() => {
  document.removeEventListener("click", closeMenu);
  if (unlistenProgress) unlistenProgress();
  if (unlistenCliImport) unlistenCliImport();
  if (unlistenDragDrop) unlistenDragDrop();
});
</script>

<template>
<div class="app">
  <header class="topbar">
    <div class="brand">
      <span class="dot"></span>
      <h1>DSH 整合包管理器</h1>
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
        <li v-for="c in envChecks" :key="c.name" class="env-item">
          <span class="env-icon" :class="c.ok ? 'ok' : 'err'">{{ c.ok ? "✓" : "✕" }}</span>
          <span class="env-name">{{ c.name }}</span>
          <span class="env-detail">{{ c.detail }}</span>
          <span class="env-critical" v-if="!c.ok && !c.critical">（非致命）</span>
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

      <!-- 版本分组 -->
      <ul class="ver-list" v-if="versionList.length">
        <li v-for="v in versionList" :key="v.version" class="ver-item">
          <div class="ver-main">
            <span class="ver-num">{{ v.version }}</span>
            <span class="badge" v-if="v.is_default">默认</span>
            <span class="badge badge-iso" v-if="v.isolated">隔离</span>
            <span class="badge badge-shared" v-else title="使用共享 home：多版本混用可能导致插件/依赖版本错配，测试版建议开隔离">共享 home</span>
            <span class="ver-meta" v-if="v.installed_at">安装于 {{ v.installed_at }}</span>
            <span class="ver-meta" v-if="verSizes[v.version] != null">占用 {{ fmtSize(verSizes[v.version]) }}</span>
          </div>
          <div class="ver-actions">
            <button class="btn primary" @click="run(v.version)" :disabled="!!runningVersion">{{ runningVersion === v.version ? "启动中..." : "运行" }}</button>
            <button class="btn" @click="openInBrowser(v.version)" title="在新终端启动并在系统浏览器打开">浏览器打开</button>
            <button class="btn" @click="setDefault(v.version)" :disabled="v.is_default">设为默认</button>
            <button class="btn" @click="createShortcut(v.version)" title="在桌面创建 DSH 快捷方式">桌面快捷方式</button>
            <button class="btn" @click="scanVersionSize(v.version)" :disabled="scanningVer === v.version" title="统计该版本占用的磁盘空间">
              {{ scanningVer === v.version ? "扫描中..." : (verSizes[v.version] != null ? "重新扫描占用" : "扫描占用") }}
            </button>
            <div class="menu-wrap" v-if="v.isolated">
              <button class="btn active" @click.stop="toggleMenu(v.version)">
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
              title="开启隔离（该版本使用独立数据目录）"
            >隔离</button>
            <button class="btn danger" @click="uninstall(v.version)" :disabled="!!uninstalling">{{ uninstalling === v.version ? "卸载中..." : "卸载" }}</button>
          </div>
        </li>
      </ul>

      <!-- 整合包分组 -->
      <div class="group-head" v-if="modpackList.length">
        <h3>整合包</h3>
        <span class="count">{{ modpackList.length }}</span>
      </div>
      <ul class="pack-list" v-if="modpackList.length">
        <li
          v-for="v in modpackList"
          :key="v.version"
          class="pack-item"
          @mouseenter="hoverItem = v.version"
          @mouseleave="hoverItem = null"
        >
          <div class="pack-icon">
            <img v-if="v.modpack_icon && /^https?:/.test(v.modpack_icon)" :src="v.modpack_icon" alt="" />
            <span v-else>📦</span>
          </div>
          <div class="pack-main">
            <div class="pack-title">
              {{ v.modpack_display_name || v.modpack_name }}
              <span class="pack-ver">{{ v.modpack_version }}</span>
              <span class="badge badge-pack" v-if="v.modpack_type === 'dshhome'">dshhome</span>
            </div>
            <div class="pack-sub">
              <span v-if="v.packed_dsh_version">DSH {{ v.packed_dsh_version }}</span>
              <span v-if="v.bundle_count != null">· {{ v.bundle_count }} 个插件</span>
              <span v-if="v.skill_count">· {{ v.skill_count }} 个技能</span>
              <span v-if="v.installed_at">· 导入于 {{ v.installed_at }}</span>
            </div>
          </div>
          <div class="ver-actions">
            <button class="btn primary" @click="run(v.version)" :disabled="!!runningVersion">{{ runningVersion === v.version ? "启动中..." : "运行" }}</button>
            <button class="btn" @click="openInBrowser(v.version)">浏览器打开</button>
            <button class="btn" @click="createShortcut(v.version)">桌面快捷方式</button>
            <button class="btn" @click="exportModpack(v.version)" title="把该实例导出为 .dspack 整合包">导出</button>
            <button class="btn" @click="scanVersionSize(v.version)" :disabled="scanningVer === v.version">
              {{ scanningVer === v.version ? "扫描中..." : (verSizes[v.version] != null ? "重新扫描占用" : "扫描占用") }}
            </button>
            <div class="menu-wrap">
              <button class="btn active" @click.stop="toggleMenu(v.version)">已隔离 ▾</button>
              <div class="menu" v-if="openMenu === v.version" @click.stop>
                <button class="menu-item" @click="openIsolatedDir(v.version)">打开数据目录</button>
                <button class="menu-item" @click="scanSize(v.version)" :disabled="scanning === v.version">
                  <span v-if="scanning === v.version">扫描中...</span>
                  <span v-else-if="sizes[v.version] != null">占用 {{ fmtSize(sizes[v.version]) }}（重新扫描）</span>
                  <span v-else>扫描占用大小</span>
                </button>
                <button class="menu-item" @click="copyShared(v.version)">复制共享数据到此</button>
                <button class="menu-item danger" @click="clearIsolated(v.version)">清理隔离数据</button>
              </div>
            </div>
            <button class="btn danger" @click="uninstall(v.version)" :disabled="!!uninstalling">{{ uninstalling === v.version ? "卸载中..." : "卸载" }}</button>
          </div>

          <!-- 悬停浮层详情 -->
          <div class="pack-pop" v-if="hoverItem === v.version">
            <div class="pop-name">{{ v.modpack_display_name || v.modpack_name }} <span class="pack-ver">{{ v.modpack_version }}</span></div>
            <div class="pop-desc" v-if="v.modpack_description">{{ v.modpack_description }}</div>
            <div class="pop-row" v-if="v.modpack_author"><b>作者</b>{{ v.modpack_author }}</div>
            <div class="pop-row"><b>标识</b>{{ v.modpack_name }}@{{ v.modpack_version }}</div>
            <div class="pop-row"><b>形态</b>{{ v.modpack_type }}</div>
            <div class="pop-row"><b>内嵌 DSH</b>{{ v.packed_dsh_version || "—" }}</div>
            <div class="pop-row"><b>插件</b>{{ v.bundle_count != null ? v.bundle_count : "—" }}</div>
            <div class="pop-row"><b>技能</b>{{ v.skill_count != null ? v.skill_count : "—" }}</div>
            <div class="pop-row" v-if="v.installed_at"><b>导入</b>{{ v.installed_at }}</div>
            <div class="pop-row" v-if="verSizes[v.version] != null"><b>占用</b>{{ fmtSize(verSizes[v.version]) }}</div>
          </div>
        </li>
      </ul>
    </section>

    <section class="panel">
      <div class="panel-head">
        <h2>导入整合包</h2>
        <button class="btn primary small" @click="pickAndPreview" :disabled="importBusy">选择 .dspack 文件</button>
      </div>
      <div class="drop-zone" :class="{ over: dragging }">
        把 .dspack 文件拖到这里，或点右上角选择文件
      </div>
      <div class="install-progress" v-if="importStage">
        <div class="spinner"></div>
        <span class="stage-text">{{ importStage }}</span>
      </div>
    </section>

    <!-- 市场 -->
    <section class="panel">
      <div class="panel-head">
        <h2 class="collapsible" @click="toggleMarket">
          <span class="caret">{{ marketCollapsed ? "▸" : "▾" }}</span>
          整合包市场
          <span class="badge-ok" v-if="marketLoaded">{{ marketPacks.length }}</span>
        </h2>
        <button class="btn small" @click.stop="loadMarket(true)" :disabled="marketLoading">
          {{ marketLoading ? "加载中..." : "刷新" }}
        </button>
      </div>

      <template v-if="!marketCollapsed">
      <div class="market-toolbar">
        <input
          class="input market-search"
          v-model="marketQuery"
          placeholder="搜索名称 / 描述 / 作者..."
        />
        <select class="input market-cat" v-model="marketCategory">
          <option value="">全部分类</option>
          <option v-for="c in marketCategories" :key="c" :value="c">{{ c }}</option>
        </select>
      </div>

      <div v-if="!marketLoaded && marketLoading" class="hint">正在加载市场索引...</div>
      <div v-else-if="marketLoaded && !filteredMarket.length" class="empty">
        {{ marketQuery || marketCategory ? "没有匹配的整合包" : "市场暂无整合包" }}
      </div>

      <ul class="market-list" v-if="filteredMarket.length">
        <li v-for="p in filteredMarket" :key="p.id" class="market-item">
          <div class="pack-icon"><span>📦</span></div>
          <div class="market-main">
            <div class="pack-title">
              {{ packDisplayName(p) }}
              <span class="pack-ver">{{ p.version }}</span>
              <span class="badge badge-installed" v-if="isPackInstalled(p)">已安装</span>
              <span class="badge badge-cat" v-if="p.category && p.category !== 'uncategorized'">{{ p.category }}</span>
            </div>
            <div class="pack-sub">
              <span v-if="p.author">by {{ p.author }}</span>
              <span v-if="p.dshVersion">· DSH {{ p.dshVersion }}</span>
              <span v-else>· DSH 自选</span>
              <span v-if="p.depCount != null">· {{ p.depCount }} 依赖</span>
              <span v-if="p.bundleCount != null">· {{ p.bundleCount }} 插件</span>
              <span>· {{ fmtBytes(p.size) }}</span>
              <span v-if="p.updatedAt">· {{ p.updatedAt }}</span>
            </div>
            <div class="market-desc" v-if="packDescription(p)">{{ packDescription(p) }}</div>
          </div>
          <div class="ver-actions">
            <button
              class="btn primary"
              @click="installFromMarket(p)"
              :disabled="!!marketInstalling"
            >{{ marketInstalling === p.id ? "安装中..." : (isPackInstalled(p) ? "再装一份" : "安装") }}</button>
          </div>
        </li>
      </ul>
      </template>
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

      <div class="chips" v-if="remote.length">
        <button
          v-for="v in remote.slice(0, 40)"
          :key="v"
          class="chip"
          @click="install(v)"
          :disabled="loading"
        >{{ v }}</button>
      </div>
      <div v-else class="hint">点击“刷新列表”从 npm 拉取所有可安装版本</div>
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
    </section>
  </main>

  <!-- 导入确认模态框 -->
  <div class="modal-mask" v-if="importPreview">
    <div class="modal">
      <h3>确认导入</h3>
      <div class="modal-pack">
        <div class="modal-icon">
          <img v-if="importPreview.icon && /^https?:/.test(importPreview.icon)" :src="importPreview.icon" alt="" />
          <span v-else>📦</span>
        </div>
        <div>
          <div class="modal-title">
            {{ importPreview.display_name }}
            <span class="pack-ver">{{ importPreview.version }}</span>
          </div>
          <div class="modal-sub">{{ importPreview.name }}@{{ importPreview.version }} · {{ importPreview.modpack_type }}</div>
        </div>
      </div>
      <div class="modal-desc" v-if="importPreview.description">{{ importPreview.description }}</div>
      <div class="modal-rows">
        <div class="pop-row" v-if="importPreview.author"><b>作者</b>{{ importPreview.author }}</div>
        <div class="pop-row"><b>容器</b>v{{ importPreview.container_version }} / manifest v{{ importPreview.manifest_version }}</div>
        <div class="pop-row"><b>目标 DSH</b>{{ importPreview.dsh_version || "（用本机最新）" }}</div>
        <div class="pop-row" v-if="importPreview.dsh_versions && importPreview.dsh_versions.length"><b>兼容集</b>{{ importPreview.dsh_versions.join(", ") }}</div>
        <div class="pop-row"><b>插件</b>{{ importPreview.bundle_count }}</div>
        <div class="pop-row"><b>依赖</b>{{ importPreview.dep_count }}</div>
        <div class="pop-row" v-if="importPreview.files_count"><b>下载资源</b>{{ importPreview.files_count }} 项</div>
        <div class="pop-row"><b>启动 profile</b>{{ importPreview.launch_profile }}</div>
        <div class="pop-row"><b>实例目录</b>{{ importPreview.instance_name }}</div>
      </div>
      <div class="modal-actions">
        <button class="btn" @click="cancelImport">取消</button>
        <button class="btn" @click="confirmImport('keep')" title="保留已有实例，新建一份">保留两份</button>
        <button class="btn" @click="confirmImport('replace')" title="删除已有同名实例后重装">重装</button>
        <button class="btn primary" @click="confirmImport('normal')">导入</button>
      </div>
    </div>
  </div>

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
.panel-head h2.collapsible { cursor: pointer; user-select: none; display: flex; align-items: center; gap: 4px; }
.panel-head h2.collapsible:hover { color: #1f2937; }
.caret { font-size: 11px; color: #8891a0; width: 12px; display: inline-block; }
.count {
  background: #eef1ff; color: #4f6ef7; font-size: 12px;
  padding: 1px 8px; border-radius: 10px; margin-left: 8px;
}

.empty, .hint { color: #9aa1ab; font-size: 13px; padding: 8px 0; }

.env-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.env-item {
  display: flex; align-items: center; gap: 10px;
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
  padding: 5px 10px; border-radius: 7px; font-size: 12px;
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

.chips { display: flex; flex-wrap: wrap; gap: 6px; max-height: 160px; overflow-y: auto; }
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

/* ---------- 整合包分组 ---------- */
.group-head {
  display: flex; align-items: center; gap: 8px;
  margin: 18px 0 8px;
}
.group-head h3 { margin: 0; font-size: 14px; color: #444c56; }

.pack-list { list-style: none; margin: 0; padding: 0; }
.pack-item {
  position: relative;
  display: flex; align-items: center; flex-wrap: wrap;
  gap: 6px 12px;
  padding: 10px 14px; margin-bottom: 8px;
  background: #fcfcfd; border: 1px solid #eceef1; border-radius: 9px;
  transition: box-shadow .15s, border-color .15s, background .15s;
}
.pack-item:hover { border-color: #c9d3e0; box-shadow: 0 2px 10px rgba(20,30,50,.08); }
.pack-icon {
  width: 36px; height: 36px; flex: 0 0 36px;
  display: flex; align-items: center; justify-content: center;
  background: #f0f3f8; border-radius: 8px; font-size: 20px; overflow: hidden;
}
.pack-icon img { width: 100%; height: 100%; object-fit: cover; }
.pack-main { flex: 1 1 auto; min-width: 0; }
.pack-title { font-weight: 600; display: flex; align-items: center; gap: 8px; }
.pack-ver { font-weight: 400; color: #6b7480; font-size: 12px; }
.pack-sub { color: #6b7480; font-size: 12px; margin-top: 2px; }
.badge-pack { background: #ede7ff; color: #6b46c1; }

/* ---------- 市场 ---------- */
.market-toolbar { display: flex; gap: 8px; margin-bottom: 10px; }
.market-search { flex: 1; min-width: 0; }
.market-cat { flex: 0 0 150px; }
.market-list { list-style: none; margin: 0; padding: 0; }
.market-item {
  display: flex; align-items: flex-start; gap: 12px;
  padding: 10px 12px; margin-bottom: 8px;
  background: #fff; border: 1px solid #e6e8eb; border-radius: 8px;
}
.market-item:hover { border-color: #c9d3e0; box-shadow: 0 2px 10px rgba(20,30,50,.08); }
.market-main { flex: 1; min-width: 0; }
.market-desc {
  color: #6b7480; font-size: 12px; margin-top: 4px; line-height: 1.5;
  display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;
  overflow: hidden;
}
.badge-installed { background: #e8f5ec; color: #2f9e5f; }
.badge-cat { background: #eef2f7; color: #5a6472; }

/* 悬停浮层 */
.pack-pop {
  position: absolute; left: 0; top: calc(100% + 4px);
  z-index: 50; width: 420px; max-width: min(420px, calc(100vw - 60px));
  max-height: 320px; overflow-y: auto;
  background: #fff; border: 1px solid #d8dee6; border-radius: 8px;
  box-shadow: 0 8px 24px rgba(20,30,50,.16);
  padding: 10px 12px; font-size: 12px; color: #333;
}
.pop-name { font-weight: 600; margin-bottom: 4px; }
.pop-desc { color: #555; margin-bottom: 6px; line-height: 1.5; }
.pop-row { display: flex; gap: 8px; line-height: 1.9; }
.pop-row b { flex: 0 0 64px; color: #8891a0; font-weight: 500; }

/* 拖拽区 */
.drop-zone {
  border: 2px dashed #ccd3dd; border-radius: 8px;
  padding: 22px; text-align: center; color: #8891a0;
  transition: border-color .15s, background .15s;
}
.drop-zone.over { border-color: #4f6ef7; background: #f2f5ff; color: #4f6ef7; }

/* 模态框 */
.modal-mask {
  position: fixed; inset: 0; z-index: 1000;
  background: rgba(20,26,36,.45);
  display: flex; align-items: center; justify-content: center;
}
.modal {
  width: 520px; max-width: 92vw; max-height: 86vh; overflow: auto;
  background: #fff; border-radius: 10px; padding: 18px 20px;
  box-shadow: 0 16px 48px rgba(10,20,40,.28);
}
.modal h3 { margin: 0 0 12px; font-size: 16px; }
.modal-pack { display: flex; gap: 12px; align-items: center; margin-bottom: 10px; }
.modal-icon {
  width: 44px; height: 44px; flex: 0 0 44px;
  display: flex; align-items: center; justify-content: center;
  background: #f0f3f8; border-radius: 8px; font-size: 22px; overflow: hidden;
}
.modal-icon img { width: 100%; height: 100%; object-fit: cover; }
.modal-title { font-weight: 600; }
.modal-sub { color: #6b7480; font-size: 12px; margin-top: 2px; }
.modal-desc { color: #555; font-size: 13px; line-height: 1.6; margin-bottom: 10px; }
.modal-rows { border-top: 1px solid #eef1f5; padding-top: 8px; margin-bottom: 14px; }
.modal-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
