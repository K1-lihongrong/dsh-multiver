<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useToast } from "../composables/useToast.js";
import NotesPanel from "./NotesPanel.vue";
import { t } from "../composables/useI18n.js";

// 可用版本列表 + 安装（含安装进度与"换源重试"弹窗）。
// state 由父组件传入（读 broken_versions 标灰）；安装前环境检查经 checkEnv 回调
// （父组件转发 <EnvCheck> 的 run）；安装/清标记后 emit refresh 让父组件重拉状态。
const props = defineProps({
  state: { type: Object, default: null },
  checkEnv: { type: Function, required: true },
});
const emit = defineEmits(["refresh", "open-url"]);

const { notify } = useToast();

const remote = ref([]);
const remoteQuery = ref("");
const remoteSortDesc = ref(true);
const loading = ref(false);
const installInput = ref("");
const installStage = ref("");
const installStep = ref(0);
const installTotal = ref(0);
const installDetail = ref("");
const installFraction = ref(0);
// 换源重试弹窗
const retryOpen = ref(false);
const retryVersion = ref("");
const retryError = ref("");
const retryRegistry = ref("https://registry.npmjs.org/");
const retryCustom = ref("");
// 错误分类："network" | "private-package" | "incomplete-version" | "unknown"
const retryKind = ref("unknown");
// 可选 npm 源（值 = registry url）。label 走 i18n，用 computed 以便切换语言时更新。
const REGISTRY_VALUES = [
  { key: "registry.official", value: "https://registry.npmjs.org/" },
  { key: "registry.aliyun", value: "https://registry.npmmirror.com" },
  { key: "registry.tencent", value: "https://mirrors.cloud.tencent.com/npm/" },
  { key: "registry.huawei", value: "https://repo.huaweicloud.com/repository/npm/" },
];
const REGISTRIES = computed(() => REGISTRY_VALUES.map((r) => ({ label: t(r.key), value: r.value })));
// 版本更新说明：当前展开说明的可用版本号（点 chip 切换）
const remoteNoteVer = ref(null);
let unlistenProgress = null;

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

/// 可用版本：按搜索词过滤 + 按排序方向排列。
const filteredRemote = computed(() => {
  const q = remoteQuery.value.trim().toLowerCase();
  let list = remote.value;
  if (q) list = list.filter((v) => v.toLowerCase().includes(q));
  // remote 本身来自 npm（默认倒序=最新在前）。升序时反转。
  if (!remoteSortDesc.value) list = [...list].reverse();
  return list;
});

async function loadRemote() {
  loading.value = true;
  try {
    remote.value = await invoke("list_remote");
  } catch (e) {
    notify(t("remote.fetchFailed") + ": " + e);
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
  return !!(props.state && props.state.broken_versions && props.state.broken_versions.includes(v));
}

/// 清除「已知安装失败」标记
async function clearBroken() {
  if (!props.state || !props.state.broken_versions || !props.state.broken_versions.length) {
    return notify(t("remote.noBrokenMarks"));
  }
  try {
    await invoke("clear_broken", { version: null });
    emit("refresh");
    notify(t("remote.clearedMarks"));
  } catch (e) {
    notify("" + e);
  }
}

/// 点可用版本 chip：填入输入框 + 在其下方展开更新说明（再点一次收起）。
function pickVersion(v) {
  installInput.value = v;
  remoteNoteVer.value = remoteNoteVer.value === v ? null : v;
}

async function install(v) {
  const ver = (v || installInput.value).trim();
  if (!ver) return notify(t("remote.enterVersion"));

  // 安装前环境检查
  loading.value = true;
  installStage.value = t("remote.checkingEnv");
  const passed = await props.checkEnv();
  if (!passed) {
    loading.value = false;
    installStage.value = "";
    notify(t("remote.envFailed"));
    return;
  }

  await doInstall(ver, null);
}

/// 执行安装（registry 为 null 时用 pnpm 默认源）。
/// 失败且疑似网络问题时，弹出「换源重试」框（不静默重试）。
async function doInstall(ver, registry) {
  loading.value = true;
  installStage.value = t("remote.preparing");
  try {
    const msg = await invoke("install_version", {
      version: ver,
      registry: registry || null,
    });
    notify(msg);
    emit("refresh");
  } catch (e) {
    const msg = String(e);
    const kind = classifyError(msg);
    // 失败可能记录了 broken_versions，刷新状态让列表标灰生效
    emit("refresh");
    retryKind.value = kind;
    retryVersion.value = ver;
    retryError.value = msg;
    retryRegistry.value = registry || "https://registry.npmjs.org/";
    retryCustom.value = "";
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

onMounted(async () => {
  // 安装进度事件由本组件消费（进度状态属于这里）
  unlistenProgress = await listen("install-progress", (e) => {
    const p = e.payload;
    installStage.value = p.stage || "";
    installStep.value = p.step || 0;
    installTotal.value = p.total || 0;
    installDetail.value = p.detail || "";
    installFraction.value = p.fraction || 0;
  });
});

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress();
});
</script>

<template>
  <section class="panel">
    <div class="panel-head">
      <h2>{{ t("remote.title") }}</h2>
      <button class="btn small" @click="loadRemote" :disabled="loading">
        {{ loading ? t("remote.loading") : t("remote.refresh") }}
      </button>
    </div>

    <div class="install-row">
      <input v-model="installInput" :placeholder="t('remote.versionPlaceholder')" @keyup.enter="install()" />
      <button class="btn primary" @click="install()" :disabled="loading">{{ t("remote.install") }}</button>
    </div>

    <NotesPanel
      v-if="remoteNoteVer"
      :version="remoteNoteVer"
      @close="remoteNoteVer = null"
      @open-url="emit('open-url', $event)"
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
      <input v-model="remoteQuery" class="input remote-search" :placeholder="t('remote.searchPlaceholder')" />
      <button class="btn small" @click="remoteSortDesc = !remoteSortDesc" :title="remoteSortDesc ? t('remote.sortDescTip') : t('remote.sortAscTip')">
        {{ remoteSortDesc ? t("remote.sortDesc") + " ↓" : t("remote.sortAsc") + " ↑" }}
      </button>
    </div>
    <div class="chips" v-if="remote.length">
      <button
        v-for="v in filteredRemote"
        :key="v"
        class="chip"
        :class="{ 'chip-broken': isBroken(v) }"
        :title="isBroken(v) ? t('remote.brokenTip') : ''"
        @click="pickVersion(v)"
        :disabled="loading"
      >{{ v }}</button>
      <div class="hint" v-if="!filteredRemote.length">{{ t("remote.noMatch") }}</div>
    </div>
    <div v-else class="hint">{{ t("remote.clickRefresh") }}</div>
    <div class="broken-hint" v-if="state && state.broken_versions && state.broken_versions.length">
      {{ t("remote.brokenHint") }}{{ state.broken_versions.join(", ") }}
      <button class="btn small" @click="clearBroken">{{ t("remote.clearBroken") }}</button>
    </div>
  </section>

  <!-- 安装失败弹窗（按错误类型分类） -->
  <div class="modal-mask" v-if="retryOpen" @click.self="retryOpen = false">
    <div class="modal">
      <!-- 网络类 -->
      <template v-if="retryKind === 'network'">
        <h3>{{ t("remote.modalNetTitle") }}</h3>
        <div class="modal-desc">
          {{ t("remote.modalNetDesc1") }} <b>{{ retryVersion }}</b> {{ t("remote.modalNetDesc2") }}
        </div>
        <div class="field">
          <label>{{ t("remote.chooseRegistry") }}</label>
          <select v-model="retryRegistry" class="input">
            <option v-for="r in REGISTRIES" :key="r.value" :value="r.value">{{ r.label }}</option>
          </select>
        </div>
        <div class="field">
          <label>{{ t("remote.customRegistry") }}</label>
          <input v-model="retryCustom" class="input" placeholder="https://.../npm/" />
        </div>
      </template>

      <!-- 私有包 / 版本不完整 -->
      <template v-else>
        <h3>{{ t("remote.modalBadTitle") }}</h3>
        <div class="modal-desc">
          {{ t("remote.modalBadDesc1") }} <b>{{ retryVersion }}</b> {{ t("remote.modalBadDesc2") }}
        </div>
        <details class="retry-detail">
          <summary>{{ t("remote.advanced") }}</summary>
          <div class="adv-body">
            <p><b>{{ t("remote.adv1") }}</b></p>
            <pre>//registry.npmjs.org/:_authToken=&lt;你的 NPM_TOKEN&gt;
@deepseek-ai:registry=https://registry.npmjs.org/</pre>
            <p>{{ t("remote.adv1b") }} <code>%USERPROFILE%\.npmrc</code>{{ t("remote.adv1c") }}</p>
            <p><b>{{ t("remote.adv2") }}</b></p>
            <pre>git clone https://github.com/deepseek-ai/deepseek-harness.git
cd deepseek-harness
pnpm install &amp;&amp; pnpm build</pre>
            <p>{{ t("remote.adv2b") }}</p>
          </div>
        </details>
      </template>

      <details class="retry-detail">
        <summary>{{ t("remote.rawError") }}</summary>
        <pre>{{ retryError }}</pre>
      </details>
      <div class="modal-actions">
        <button class="btn" @click="retryOpen = false">{{ retryKind === 'network' ? t("remote.cancel") : t("remote.gotIt") }}</button>
        <button v-if="retryKind === 'network'" class="btn primary" @click="retryInstall">{{ t("remote.retryWithRegistry") }}</button>
        <button v-else class="btn primary" @click="retryOpen = false">{{ t("remote.ok") }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.install-row { display: flex; gap: 8px; margin-bottom: 12px; }
.install-row input {
  flex: 1; padding: 8px 12px; border: 1px solid var(--border);
  border-radius: 7px; font-size: 13px; font-family: inherit; outline: none;
}
.install-row input:focus { border-color: var(--accent); }

.install-progress {
  display: flex; align-items: flex-start; gap: 10px;
  padding: 10px 14px; margin-bottom: 12px;
  background: var(--accent-weak-2); border: 1px solid var(--accent-weak-3); border-radius: 8px;
}
.prog-body { flex: 1; min-width: 0; }
.prog-line { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
.prog-detail { font-size: 12px; color: var(--text-3); }
.prog-count { font-size: 12px; color: var(--text-muted); margin-left: auto; font-variant-numeric: tabular-nums; }
.prog-bar { height: 6px; background: var(--accent-weak-3); border-radius: 3px; overflow: hidden; }
.prog-fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width .3s ease; }
.spinner {
  width: 15px; height: 15px; flex-shrink: 0;
  border: 2px solid var(--accent-border-2); border-top-color: var(--accent);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
.stage-text { font-size: 13px; color: var(--accent); }

.remote-toolbar { display: flex; gap: 8px; margin-bottom: 10px; }
.remote-search { flex: 1; min-width: 0; padding: 6px 12px; border: 1px solid var(--border); border-radius: 7px; font-size: 13px; font-family: inherit; outline: none; }
.remote-search:focus { border-color: var(--accent); }
.chips { display: flex; flex-wrap: wrap; gap: 6px; max-height: 120px; overflow-y: auto; }
.chip {
  border: 1px solid var(--surface-5); background: var(--surface-1); color: var(--text-code);
  padding: 4px 10px; border-radius: 6px; font-size: 12px;
  cursor: pointer; transition: all .12s; font-family: inherit;
  font-variant-numeric: tabular-nums;
}
.chip:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); background: var(--accent-weak-2); }
.chip:disabled { opacity: .5; cursor: not-allowed; }
.chip-broken {
  opacity: .5; text-decoration: line-through;
  border-style: dashed; cursor: not-allowed;
}
.broken-hint {
  margin-top: 10px; font-size: 12px; color: var(--text-muted);
  display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
}

/* 换源重试弹窗 */
.modal-mask {
  position: fixed; inset: 0; z-index: 1000;
  background: var(--shadow-strong);
  display: flex; align-items: center; justify-content: center;
}
.modal {
  width: 480px; max-width: 92vw; max-height: 86vh; overflow: auto;
  background: var(--panel-bg); border-radius: 10px; padding: 18px 20px;
  box-shadow: 0 16px 48px var(--overlay);
}
.modal h3 { margin: 0 0 10px; font-size: 15px; }
.modal-desc { font-size: 13px; color: var(--text-code); margin-bottom: 14px; line-height: 1.5; }
.retry-detail { margin-bottom: 12px; font-size: 12px; color: var(--text-3); }
.retry-detail summary { cursor: pointer; }
.adv-body { margin-top: 6px; line-height: 1.6; }
.adv-body p { margin: 8px 0 4px; }
.adv-body pre {
  margin: 4px 0; padding: 8px; background: var(--gray-hover); border-radius: 6px;
  font-size: 11px; white-space: pre-wrap; word-break: break-all;
}
.adv-body code { background: var(--gray-chip); padding: 1px 4px; border-radius: 3px; }
.retry-detail pre {
  margin: 6px 0 0; padding: 8px; max-height: 160px; overflow: auto;
  background: var(--gray-hover); border-radius: 6px; font-size: 11px;
  white-space: pre-wrap; word-break: break-all;
}
.modal-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
