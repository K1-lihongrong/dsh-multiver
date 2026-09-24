<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const state = ref(null);
const installed = ref([]);
const remote = ref([]);
const loading = ref(false);
const toast = ref("");
const toastExpanded = ref(false);
const installInput = ref("");
const rootInput = ref("");
const installStage = ref("");
const envChecks = ref([]);
const envChecked = ref(false);
const envPassed = ref(false);
let toastTimer = null;
let unlistenProgress = null;

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
  if (!confirm("确定卸载版本 " + v + " ?")) return;
  try {
    notify(await invoke("uninstall_version", { version: v }));
    await refresh();
  } catch (e) { notify("" + e); }
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

async function run(v) {
  try {
    notify(await invoke("run_version", { version: v }));
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
const scanning = ref(null);       // 正在扫描的版本号
const sizes = ref({});            // 版本号 -> 字节数

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
  if (!confirm("将把共享数据（<根>/home）复制到该版本的隔离目录。\n已存在的文件不会覆盖。\n\n继续吗？")) return;
  try {
    notify(await invoke("copy_shared_to_isolated", { version: v }));
    sizes.value = { ...sizes.value, [v]: undefined };
  } catch (e) { notify("" + e); }
}

async function clearIsolated(v) {
  closeMenu();
  if (!confirm("将删除该版本的隔离数据（保留版本本身）。\n此操作不可恢复，继续吗？")) return;
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
  unlistenProgress = await listen("install-progress", (e) => {
    installStage.value = e.payload;
  });
  await runEnvCheck();
  document.addEventListener("click", closeMenu);
});

onUnmounted(() => {
  document.removeEventListener("click", closeMenu);
  if (unlistenProgress) unlistenProgress();
});
</script>

<template>
<div class="app">
  <header class="topbar">
    <div class="brand">
      <span class="dot"></span>
      <h1>DSH 版本管理器</h1>
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

      <ul class="ver-list" v-else>
        <li v-for="v in installed" :key="v.version" class="ver-item">
          <div class="ver-main">
            <span class="ver-num">{{ v.version }}</span>
            <span class="badge" v-if="v.is_default">默认</span>
            <span class="badge badge-iso" v-if="v.isolated">隔离</span>
          </div>
          <div class="ver-actions">
            <button class="btn primary" @click="run(v.version)">运行</button>
            <button class="btn" @click="setDefault(v.version)" :disabled="v.is_default">设为默认</button>
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
            <button class="btn danger" @click="uninstall(v.version)">卸载</button>
          </div>
        </li>
      </ul>
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
        <span class="stage-text">{{ installStage }}</span>
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
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 14px; border: 1px solid #eceef1; border-radius: 9px;
  background: #fcfcfd; transition: border-color .15s, background .15s;
}
.ver-item:hover { border-color: #d6dae0; background: #fff; }
.ver-main { display: flex; align-items: center; gap: 10px; }
.ver-num { font-weight: 600; font-size: 14px; font-variant-numeric: tabular-nums; }
.badge {
  background: #e8f5ec; color: #2f9e5f; font-size: 11px;
  padding: 2px 8px; border-radius: 10px; font-weight: 500;
}
.badge-iso { background: #fff4e5; color: #c77d1a; }
.btn.active {
  background: #fff4e5; border-color: #f0c98a; color: #c77d1a;
}
.btn.active:hover:not(:disabled) { background: #ffedcc; border-color: #e0b46a; }
.ver-actions { display: flex; gap: 8px; align-items: center; }

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
  display: flex; align-items: center; gap: 10px;
  padding: 10px 14px; margin-bottom: 12px;
  background: #f5f7ff; border: 1px solid #e2e7ff; border-radius: 8px;
}
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
</style>
