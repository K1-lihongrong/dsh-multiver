<script setup>
import { ref, computed, watch, onMounted, onUnmounted, provide, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";
import { DS_PLUGIN_LOG_KEY, useDsPluginLog } from "../composables/useDsPluginLog.js";
import DsPluginInstalled from "./DsPluginInstalled.vue";
import DsPluginCatalog from "./DsPluginCatalog.vue";
import DsPluginInstallDialog from "./DsPluginInstallDialog.vue";
import DsPluginMeta from "./DsPluginMeta.vue";

// v0.3.0 双向桥 B1：管理某 dsh 版本的插件（dsh profile 里的 npm 包）。
// 通过 dsh CLI（dsh plugin --profile <p> add/remove <包>）执行。
//
// v0.3.3 D21：本文件只做壳——面板头部、工具栏、标签切换、日志、profile 加载。
// 四类职责已拆到子组件：
//   已安装列表 → DsPluginInstalled.vue
//   安装对话框 → DsPluginInstallDialog.vue
//   版本豁免与构建脚本审批 → DsPluginMeta.vue
//   推荐页 → DsPluginCatalog.vue
const props = defineProps({
  version: { type: String, required: true },
});
const emit = defineEmits(["close"]);

const { notify } = useToast();
const profiles = ref([]);
const dshHome = ref("");
const homeKind = ref("shared");
const loading = ref(false);
const busy = ref(false);
const activeProfile = ref("");
const installing = ref(false);

// B1-04/B1-08：进度输出（实时行）、初始化（B1-07）
const { logLines, showLog } = useDsPluginLog();
const initName = ref("web");
const initializing = ref(false);
let unlistenProgress = null;

// 进度行同时被安装对话框（installing 阶段）与面板底部日志区使用
provide(DS_PLUGIN_LOG_KEY, { logLines, showLog });

// 阶段 4：已安装 / 推荐 两个标签页
const tab = ref("installed"); // installed | catalog

const homeKindLabel = computed(() => {
  const m = { isolated: "dsplugins.homeIsolated", official: "dsplugins.homeOfficial", shared: "dsplugins.homeShared" };
  return t(m[homeKind.value] || "dsplugins.homeShared");
});
const homeKindClass = computed(() => "kind-" + homeKind.value);

// B1-11/B1-12：profile 级官方信息（运行时版本 / 版本豁免 / 待决定构建脚本 / 日志目录）
const profileInfo = ref({ runtime_version: null, exemptions: {}, pending_builds: [], logs_dir: "" });
const lastAction = ref(null);

const currentProfile = () => profiles.value.find((p) => p.name === activeProfile.value);

/// 已安装的插件名集合，用于在推荐列表上标「已安装」。
const installedNames = computed(() => {
  const set = new Set();
  for (const pl of currentProfile()?.plugins || []) set.add(pl.name);
  return set;
});

async function refreshInfo() {
  if (!activeProfile.value) {
    profileInfo.value = { runtime_version: null, exemptions: {}, pending_builds: [], logs_dir: "" };
    return;
  }
  try {
    const info = await invoke("ds_profile_info", {
      version: props.version,
      profile: activeProfile.value,
    });
    profileInfo.value = {
      runtime_version: info?.runtime_version || null,
      exemptions: info?.exemptions || {},
      pending_builds: info?.pending_builds || [],
      logs_dir: info?.logs_dir || "",
    };
  } catch (_) {
    // 非 Tauri / 旧后端：保持空信息
  }
}

async function load() {
  loading.value = true;
  try {
    const res = await invoke("ds_list_plugins", { version: props.version });
    profiles.value = res?.profiles || [];
    dshHome.value = res?.dsh_home || "";
    homeKind.value = res?.home_kind || "shared";
    if (!activeProfile.value && profiles.value.length) {
      activeProfile.value = profiles.value[0].name;
    }
    await refreshInfo();
  } catch (e) {
    notify("" + e);
  } finally {
    loading.value = false;
  }
}

/// 切换标签。推荐页首次进入时由子组件自行加载（active 变 true）。
function switchTab(next) {
  tab.value = next;
}

const installedRef = ref(null);
const installRef = ref(null);

/// 打开安装对话框。从推荐页点「安装」时带上该条目的 spec。
function openInstall(prefill) {
  installRef.value?.open(prefill);
}

/// 列表变化后重新拉取（安装、卸载、更新都走这里）。
async function reload() {
  await load();
  await refreshInfo();
}

/// 已安装列表里某次操作失败 → 记下，等「允许构建脚本」后重试。
function onActionFailed(last) {
  lastAction.value = last;
}
function onActionDone() {
  lastAction.value = null;
}

/// B1-12：构建脚本审批通过后重试上次失败的操作。
async function retryLastAction() {
  const last = lastAction.value;
  if (!last) return;
  lastAction.value = null;
  // 已安装页按需挂载：当前不在该标签时先切过去，等 DOM 就绪再执行
  if (tab.value !== "installed") {
    tab.value = "installed";
    await nextTick();
  }
  await installedRef.value?.runAction(last.action, last.name, last.bundle);
}

async function openLogs() {
  try {
    await invoke("ds_open_plugin_logs", { version: props.version, profile: activeProfile.value });
  } catch (e) {
    notify("" + e);
  }
}

// B1-07：无 profile 时初始化（dsh 首次使用会自动创建 profile 目录）
async function initProfile() {
  const name = initName.value.trim();
  if (!name || initializing.value) return;
  initializing.value = true;
  logLines.value = [];
  try {
    await invoke("ds_init_profile", { version: props.version, profile: name });
    notify(t("dsplugins.initDone", { p: name }));
    if (logLines.value.length) showLog.value = true;
    activeProfile.value = name;
    await load();
  } catch (e) {
    notify("" + e);
    showLog.value = true;
  } finally {
    initializing.value = false;
  }
}

async function openDir() {
  try {
    await invoke("ds_open_profile_dir", { version: props.version, profile: activeProfile.value });
  } catch (e) {
    notify("" + e);
  }
}

onMounted(async () => {
  await load();
  try {
    unlistenProgress = await listen("ds-plugin-progress", (e) => {
      const p = e?.payload;
      if (!p || p.version !== props.version) return;
      logLines.value.push(p.line);
      if (logLines.value.length > 500) logLines.value.splice(0, logLines.value.length - 500);
    });
  } catch (_) {
    // 非 Tauri 环境（E2E mock）忽略
  }
});

// 切换 profile 时刷新官方信息（豁免 / 待决定构建脚本 / 日志目录）
watch(activeProfile, () => {
  refreshInfo();
});

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress();
});
</script>

<template>
  <div class="ds-plugin-panel">
    <!-- 头部：标题 + DSH_HOME + 操作 -->
    <div class="dp-head">
      <div class="dp-head-left">
        <span class="dp-title">{{ t("dsplugins.title", { v: version }) }}</span>
        <span class="dp-home" :class="homeKindClass" :title="dshHome">
          <span class="dp-home-kind">{{ homeKindLabel }}</span>
          <code class="dp-home-path">{{ dshHome }}</code>
        </span>
      </div>
      <div class="dp-head-actions">
        <button class="dp-icon-btn" @click="load" :disabled="loading" :title="t('dsplugins.refresh')">↻</button>
        <button class="dp-icon-btn" @click="openLogs" :disabled="!activeProfile" :title="t('dsplugins.logs')">📜</button>
        <button class="dp-icon-btn" @click="openDir" :disabled="!activeProfile" :title="t('dsplugins.openDir')">📂</button>
        <button class="dp-icon-btn" @click="emit('close')" :title="t('dsplugins.close')">✕</button>
      </div>
    </div>

    <div class="dp-hint">{{ t("dsplugins.hint") }}</div>

    <!-- 加载 / 空 / 内容 -->
    <div v-if="loading" class="dp-state">
      <span class="dp-spinner"></span>
      <span>{{ t("dsplugins.loading") }}</span>
    </div>
    <div v-else-if="!profiles.length" class="dp-state dp-state-empty dp-init">
      <div class="dp-init-text">{{ t("dsplugins.noProfile") }}</div>
      <div class="dp-add">
        <input
          class="dp-input"
          v-model="initName"
          :placeholder="t('dsplugins.initPlaceholder')"
          :disabled="initializing"
          @keyup.enter="initProfile"
        />
        <button class="dp-btn dp-btn-primary" @click="initProfile" :disabled="initializing || !initName.trim()">
          {{ initializing ? t("dsplugins.initializing") : t("dsplugins.init") }}
        </button>
      </div>
    </div>
    <template v-else>
      <!-- profile 切换（多个时） + 标签切换 + 操作 -->
      <div class="dp-toolbar">
        <select v-if="profiles.length > 1" class="dp-select" :value="activeProfile" @change="activeProfile = $event.target.value">
          <option v-for="p in profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
        <span v-else class="dp-profile-tag">{{ activeProfile }}</span>

        <!-- 视图切换：与右侧操作按钮在视觉上分开（连体分段控件 vs 独立按钮） -->
        <div class="dp-segmented" role="tablist">
          <button
            class="dp-seg-btn"
            :class="{ active: tab === 'installed' }"
            role="tab"
            :aria-selected="tab === 'installed'"
            @click="switchTab('installed')"
          >{{ t("dsplugins.tabInstalled") }}</button>
          <button
            class="dp-seg-btn"
            :class="{ active: tab === 'catalog' }"
            role="tab"
            :aria-selected="tab === 'catalog'"
            @click="switchTab('catalog')"
          >{{ t("dsplugins.tabCatalog") }}</button>
        </div>

        <button class="dp-btn dp-btn-primary" @click="openInstall()" :disabled="busy">
          ＋ {{ t("dsplugins.addPlugin") }}
        </button>
        <button
          v-if="tab === 'installed'"
          class="dp-btn"
          @click="installedRef?.checkUpdates()"
          :disabled="busy"
        >
          {{ t("dsplugins.checkUpdates") }}
        </button>
      </div>

      <!-- 已安装标签页 -->
      <DsPluginInstalled
        v-if="tab === 'installed'"
        ref="installedRef"
        :version="version"
        :profile="activeProfile"
        :plugins="currentProfile()?.plugins || []"
        v-model:busy="busy"
        @reload="reload"
        @refresh-info="refreshInfo"
        @action-failed="onActionFailed"
        @action-done="onActionDone"
      />

      <!-- 推荐标签页 -->
      <DsPluginCatalog
        v-if="tab === 'catalog'"
        :active="tab === 'catalog'"
        :installed-names="installedNames"
        :installing="installing"
        :busy="busy"
        @install="openInstall"
      />

      <!-- B1-08：操作输出（实时进度行） -->
      <div class="dp-log" v-if="logLines.length">
        <button class="dp-log-toggle" @click="showLog = !showLog">
          {{ showLog ? t("dsplugins.hideLog") : t("dsplugins.showLog") }} ({{ logLines.length }})
        </button>
        <pre class="dp-log-body" v-show="showLog">{{ logLines.join("\n") }}</pre>
      </div>

      <!-- B1-11/B1-12：官方信息（待决定构建脚本 / 版本豁免） -->
      <DsPluginMeta
        :version="version"
        :profile="activeProfile"
        :profile-info="profileInfo"
        :busy="busy"
        @refresh-info="refreshInfo"
        @approved="retryLastAction"
      />
    </template>

    <!-- 官方风格安装对话框：form → confirm → installing → done/failed -->
    <DsPluginInstallDialog
      ref="installRef"
      :version="version"
      :profile="activeProfile"
      :profile-info="profileInfo"
      :plugins="currentProfile()?.plugins || []"
      @installed="reload"
      @refresh-info="refreshInfo"
      @installing-change="installing = $event"
    />
  </div>
</template>
