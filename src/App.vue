<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import NotesPanel from "./components/NotesPanel.vue";
import EnvCheck from "./components/EnvCheck.vue";
import { useToast } from "./composables/useToast.js";
import PathSettings from "./components/PathSettings.vue";
import InstalledList from "./components/InstalledList.vue";

const state = ref(null);
const installed = ref([]);
const remote = ref([]);
const remoteQuery = ref("");      // 可用版本搜索词
const remoteSortDesc = ref(true); // 排序：true=最新在前
const loading = ref(false);
// toast（通知）由 useToast 提供（模块级单例，与拆出的组件共享）
const { toast, toastExpanded, notify, dismissToast, toggleToast } = useToast();
const installInput = ref("");
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
/// 可用版本：按搜索词过滤 + 按排序方向排列。
const filteredRemote = computed(() => {
  const q = remoteQuery.value.trim().toLowerCase();
  let list = remote.value;
  if (q) list = list.filter((v) => v.toLowerCase().includes(q));
  // remote 本身来自 npm（默认倒序=最新在前）。升序时反转。
  if (!remoteSortDesc.value) list = [...list].reverse();
  return list;
});
const envCheckRef = ref(null);  // <EnvCheck> 组件引用（安装前调 run()）
let unlistenProgress = null;
let unlistenCrash = null;

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

async function refresh() {
  state.value = await invoke("get_state");
  installed.value = await invoke("list_installed");
}

/// 调 EnvCheck 组件跑一次环境检查（安装前用），返回是否通过。
async function runEnvCheck() {
  return envCheckRef.value ? envCheckRef.value.run() : false;
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

// ===== 版本更新说明（数据源：dsh 上游 GitHub releases）=====
// 拉取 / 消毒 / 渲染都在 <NotesPanel> 组件内部；这里只保留"展开哪个版本"的状态。
// 注意：ref 必须在引用它的函数定义**之前**声明（否则函数调用时撞 const 的 TDZ）。
const remoteNoteVer = ref(null);    // 可用版本里当前展开说明的版本号

/// 点可用版本 chip：填入输入框 + 在其下方展开更新说明（再点一次收起）。
function pickVersion(v) {
  installInput.value = v;
  remoteNoteVer.value = remoteNoteVer.value === v ? null : v;
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

async function clearDefault() {
  try {
    notify(await invoke("set_default", { version: null }));
    await refresh();
  } catch (e) { notify("" + e); }
}

async function openUrl(url) {
  try { await invoke("open_url", { url }); }
  catch (e) { notify("打开链接失败：" + e); }
}

async function openDir(which) {
  try { await invoke("open_dir", { which }); }
  catch (e) { notify("" + e); }
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
});

onUnmounted(() => {
  window.removeEventListener("error", onWindowError);
  window.removeEventListener("unhandledrejection", onUnhandledRejection);
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
    <EnvCheck ref="envCheckRef" @open-url="openUrl" @error="notify" />

    <InstalledList
      :installed="installed"
      @refresh="refresh"
      @open-url="openUrl"
    />

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

      <NotesPanel
        v-if="remoteNoteVer"
        :version="remoteNoteVer"
        @close="remoteNoteVer = null"
        @open-url="openUrl"
      />

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

    <PathSettings
      :state="state"
      :default-version="state && state.default_version"
      @refresh="refresh"
      @open-dir="openDir"
      @clear-default="clearDefault"
    />
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
