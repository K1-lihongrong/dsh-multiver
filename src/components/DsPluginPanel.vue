<script setup>
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";

// v0.3.0 双向桥 B1：管理某 dsh 版本的插件（dsh profile 里的 npm 包）。
// 通过 dsh CLI（dsh plugin --profile <p> add/remove <包>）执行。
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
// B1-13/B1-14 + 官方安装对话框：安装弹窗 / 安装源 / 更新检查 / 版本豁免「允许」
const showInstall = ref(false);
const installSpec = ref("");
const installRegistry = ref(""); // ""=默认（pnpm 配置）, official, mirror, custom
const customRegistry = ref("");
const installing = ref(false);
const updates = ref({});
const allowSpec = ref("");
const allowRuntime = ref("");
const allowRisk = ref(false);
// B1-04/B1-08：进度输出（实时行）、搜索（B1-10）、初始化（B1-07）
const logLines = ref([]);
const showLog = ref(false);
const search = ref("");
const initName = ref("web");
const initializing = ref(false);
let unlistenProgress = null;

const homeKindLabel = computed(() => {
  const m = { isolated: "dsplugins.homeIsolated", official: "dsplugins.homeOfficial", shared: "dsplugins.homeShared" };
  return t(m[homeKind.value] || "dsplugins.homeShared");
});
const homeKindClass = computed(() => "kind-" + homeKind.value);

// v0.3.0 B1：与 dsh 官方插件页语义对齐。
// 「随 DSH 提供」= 官方安装随附的 bundle（OPTIONAL/DEFAULT/ON_DEMAND，后端从 dsh-app-boot 读取）；
// 「已安装」= profile dependencies；「插件 / 依赖」= 包是否声明 dsh.bundle（官方定义）。
const srcOf = (pl) => pl.source || (pl.is_bundle ? "bundle" : "user");
const isBundle = (pl) => srcOf(pl) === "bundle";
const isOfficial = (pl) => pl.official === true;
const srcLabel = (pl) => t(isOfficial(pl) ? "dsplugins.sourceBundle" : "dsplugins.sourceUser");
const kindLabel = (pl) => t(pl.declares_bundle ? "dsplugins.kindPlugin" : "dsplugins.kindDependency");

// B1-11/B1-12：profile 级官方信息（运行时版本 / 版本豁免 / 待决定构建脚本 / 日志目录）
const profileInfo = ref({ runtime_version: null, exemptions: {}, pending_builds: [], logs_dir: "" });
const lastAction = ref(null);
const exemptionCount = computed(() => Object.keys(profileInfo.value.exemptions || {}).length);

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

// 官方 CLI 的失败诊断对齐：把官方提示（allow-version / pnpm 缺失 / 诊断日志路径）转成本地化提示。
function actionErrorText(e) {
  const msg = String(e || "");
  const extra = [];
  if (msg.includes("allow-version") || msg.includes("incompatible-version")) {
    extra.push(t("dsplugins.incompatibleHint"));
  }
  if (msg.includes("pnpm was not found")) extra.push(t("dsplugins.pnpmMissingHint"));
  const m = msg.match(/diagnostics:\s*(.+)/);
  if (m) extra.push(t("dsplugins.diagnosticsHint", { p: m[1].trim() }));
  return extra.length ? msg + "\n\n" + extra.join("\n") : msg;
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

const currentProfile = () => profiles.value.find((p) => p.name === activeProfile.value);

// B1-10：按包名/版本过滤
const filteredPlugins = computed(() => {
  const list = currentProfile()?.plugins || [];
  const q = search.value.trim().toLowerCase();
  if (!q) return list;
  return list.filter(
    (pl) =>
      pl.name.toLowerCase().includes(q) ||
      String(pl.version || "").toLowerCase().includes(q),
  );
});

async function doAction(action, pkg, locked) {
  const name = (pkg || "").trim();
  if (!name) return;
  // 仅 DSH 随附的官方组合包不允许单独卸载；用户安装的 bundle 可卸载。
  if (action === "remove" && locked) {
    notify(t("dsplugins.bundleLocked"));
    return;
  }
  // 卸载是破坏性操作：先二次确认（与项目其他卸载操作一致）。
  if (action === "remove") {
    const ok = await ask(t("dsplugins.removeConfirm", { p: name }), {
      title: t("dsplugins.remove"),
      kind: "warning",
    });
    if (!ok) return;
  }
  busy.value = true;
  logLines.value = [];
  try {
    await invoke("ds_plugin_action", {
      version: props.version,
      profile: activeProfile.value,
      action,
      package: name,
    });
    notify(t(action === "add" ? "dsplugins.installed" : "dsplugins.removed", { p: name }));
    lastAction.value = null;
    if (logLines.value.length) showLog.value = true;
    await load();
  } catch (e) {
    lastAction.value = { action, name, bundle: !!bundle };
    notify(actionErrorText(e));
    showLog.value = true;
    await refreshInfo();
  } finally {
    busy.value = false;
  }
}

// ── 官方安装对话框（对齐 dsh 插件页：form → confirm → installing → done/failed）──
const installPhase = ref("form");
const installResult = ref(null);
const installError = ref("");
const inspectInfo = ref(null);
const inspectFailed = ref(false);
const registryProbed = ref(false);
const REG_KEY = "dsh-multiver.installRegistry";
const REG_CUSTOM_KEY = "dsh-multiver.installRegistryCustom";

const registryValue = computed(() => {
  if (installRegistry.value === "official") return "https://registry.npmjs.org";
  if (installRegistry.value === "mirror") return "https://registry.npmmirror.com";
  if (installRegistry.value === "custom") return customRegistry.value.trim() || null;
  return null;
});

const registryLabelText = computed(() => {
  if (installRegistry.value === "official") return t("dsplugins.registryOfficial");
  if (installRegistry.value === "mirror") return t("dsplugins.registryMirror");
  if (installRegistry.value === "custom") return customRegistry.value.trim() || t("dsplugins.registryCustom");
  return t("dsplugins.registryDefault");
});

const completedPlugin = computed(() => {
  const name = installResult.value?.name;
  if (!name) return null;
  return currentProfile()?.plugins?.find((p) => p.name === name) || null;
});

async function openInstall() {
  showInstall.value = true;
  installSpec.value = "";
  installPhase.value = "form";
  installResult.value = null;
  installError.value = "";
  inspectInfo.value = null;
  inspectFailed.value = false;
  registryProbed.value = false;
  logLines.value = [];
  try {
    installRegistry.value = localStorage.getItem(REG_KEY) || "";
    customRegistry.value = localStorage.getItem(REG_CUSTOM_KEY) || "";
  } catch (_) {
    installRegistry.value = "";
    customRegistry.value = "";
  }
  // 首次使用（未记住安装源）时探测最快源（对齐官方 registryProbe）
  if (!installRegistry.value) {
    try {
      const p = await invoke("ds_probe_registries");
      if (p?.kind) {
        installRegistry.value = p.kind;
        registryProbed.value = true;
      }
    } catch (_) {
      // 探测失败保持默认
    }
  }
}

function inspectProblemText(info) {
  const map = {
    "not-a-bundle": "dsplugins.inspectNotBundle",
    "not-found": "dsplugins.inspectNotFound",
    network: "dsplugins.inspectNetwork",
  };
  const key = map[info?.problem] || "dsplugins.inspectProblem";
  const msg = t(key, { p: info?.name || installSpec.value });
  return info?.detail ? msg + "\n" + info.detail : msg;
}

async function requestInstall() {
  const spec = installSpec.value.trim();
  if (!spec) return;
  if (installRegistry.value === "custom" && !customRegistry.value.trim()) {
    notify(t("dsplugins.customRegistryRequired"));
    return;
  }
  // B1-16：安装前 inspect（对齐官方：先确认 spec 指向什么）
  installPhase.value = "checking";
  installError.value = "";
  inspectFailed.value = false;
  inspectInfo.value = null;
  try {
    const info = await invoke("ds_inspect_spec", {
      version: props.version,
      profile: activeProfile.value,
      spec,
      registry: registryValue.value,
    });
    inspectInfo.value = info || null;
    if (info?.problem) {
      installError.value = inspectProblemText(info);
      inspectFailed.value = true;
      installPhase.value = "failed";
      return;
    }
    installPhase.value = "confirm";
  } catch (e) {
    installError.value = String(e);
    inspectFailed.value = true;
    installPhase.value = "failed";
  }
}

function retryInstall() {
  if (inspectFailed.value || !inspectInfo.value) {
    requestInstall();
  } else {
    confirmInstall();
  }
}

// 失败界面内直接允许 pnpm 拦截的构建脚本并重试安装（对齐官方）
async function approveAndRetryInstall() {
  const pkgs = profileInfo.value.pending_builds || [];
  if (!pkgs.length) return;
  try {
    await invoke("ds_approve_builds", {
      version: props.version,
      profile: activeProfile.value,
      packages: pkgs,
    });
    await refreshInfo();
    retryInstall();
  } catch (e) {
    notify("" + e);
  }
}

async function confirmInstall() {
  const spec = installSpec.value.trim();
  if (!spec || installing.value) return;
  try {
    localStorage.setItem(REG_KEY, installRegistry.value);
    localStorage.setItem(REG_CUSTOM_KEY, customRegistry.value);
  } catch (_) {
    // localStorage 不可用时忽略
  }
  installPhase.value = "installing";
  installError.value = "";
  installResult.value = null;
  installing.value = true;
  logLines.value = [];
  try {
    const res = await invoke("ds_plugin_action", {
      version: props.version,
      profile: activeProfile.value,
      action: "add",
      package: spec,
      registry: registryValue.value,
    });
    installResult.value = { name: res?.name || null, spec, registry: registryValue.value };
    installPhase.value = "done";
    await load();
    await refreshInfo();
  } catch (e) {
    installError.value = actionErrorText(e);
    installPhase.value = "failed";
    await refreshInfo();
  } finally {
    installing.value = false;
  }
}

function backToForm() {
  installPhase.value = "form";
}

function closeInstall() {
  showInstall.value = false;
}

function useExample() {
  installSpec.value = t("dsplugins.exampleSpec");
}

// B1-13：检查可更新插件（dsh CLI `outdated --json`）
async function checkUpdates() {
  if (!activeProfile.value) return;
  busy.value = true;
  try {
    const list = await invoke("ds_check_updates", {
      version: props.version,
      profile: activeProfile.value,
    });
    const map = {};
    for (const u of list || []) map[u.name] = u;
    updates.value = map;
    notify(t("dsplugins.updatesFound", { n: Object.keys(map).length }));
  } catch (e) {
    notify("" + e);
  } finally {
    busy.value = false;
  }
}

// B1-13：更新 = 官方语义「先卸载再安装新版」
async function updatePlugin(pl) {
  const u = updates.value[pl.name];
  if (!u) return;
  const ok = await ask(t("dsplugins.updateConfirm", { p: pl.name, v: u.latest }), {
    title: t("dsplugins.update"),
    kind: "warning",
  });
  if (!ok) return;
  busy.value = true;
  logLines.value = [];
  try {
    await invoke("ds_plugin_action", {
      version: props.version,
      profile: activeProfile.value,
      action: "remove",
      package: pl.name,
    });
    await invoke("ds_plugin_action", {
      version: props.version,
      profile: activeProfile.value,
      action: "add",
      package: `${pl.name}@${u.latest}`,
    });
    notify(t("dsplugins.updated", { p: pl.name, v: u.latest }));
    await load();
    await checkUpdates();
  } catch (e) {
    notify(actionErrorText(e));
    showLog.value = true;
  } finally {
    busy.value = false;
  }
}

// B1-14：允许版本不兼容（官方 allow-version --accept-risk）
async function allowExemption() {
  const spec = allowSpec.value.trim();
  const runtime = (allowRuntime.value || profileInfo.value.runtime_version || "").trim();
  if (!spec || !runtime || !allowRisk.value) return;
  try {
    await invoke("ds_exemption_action", {
      version: props.version,
      profile: activeProfile.value,
      action: "allow",
      package_version: spec,
      dsh_version: runtime,
    });
    notify(t("dsplugins.exemptionAllowed", { p: spec }));
    allowSpec.value = "";
    allowRisk.value = false;
    await refreshInfo();
  } catch (e) {
    notify("" + e);
  }
}

// B1-12：允许 pnpm 拦截的依赖构建脚本（写 pnpm-workspace.yaml 的 allowBuilds），成功后重试上次操作
async function approveBuilds() {
  const pkgs = profileInfo.value.pending_builds || [];
  if (!pkgs.length) return;
  busy.value = true;
  try {
    const left = await invoke("ds_approve_builds", {
      version: props.version,
      profile: activeProfile.value,
      packages: pkgs,
    });
    profileInfo.value.pending_builds = left || [];
    notify(t("dsplugins.buildsApproved", { n: pkgs.length }));
    const last = lastAction.value;
    if (last) {
      lastAction.value = null;
      await doAction(last.action, last.name, last.bundle);
    }
  } catch (e) {
    notify("" + e);
  } finally {
    busy.value = false;
  }
}

// B1-12：撤销版本豁免（官方 CLI revoke-version）
async function revokeExemption(spec, runtime) {
  const ok = await ask(t("dsplugins.exemptionRevokeConfirm", { p: spec }), {
    title: t("dsplugins.exemptions"),
    kind: "warning",
  });
  if (!ok) return;
  try {
    await invoke("ds_exemption_action", {
      version: props.version,
      profile: activeProfile.value,
      action: "revoke",
      package_version: spec,
      dsh_version: runtime,
    });
    notify(t("dsplugins.exemptionRevoked", { p: spec }));
    await refreshInfo();
  } catch (e) {
    notify("" + e);
  }
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
      <!-- profile 切换（多个时） -->
      <div class="dp-toolbar">
        <select v-if="profiles.length > 1" class="dp-select" :value="activeProfile" @change="activeProfile = $event.target.value">
          <option v-for="p in profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
        <span v-else class="dp-profile-tag">{{ activeProfile }}</span>

        <input v-model="search" class="dp-input dp-search" :placeholder="t('dsplugins.searchPlaceholder')" />

        <button class="dp-btn dp-btn-primary" @click="openInstall" :disabled="busy">
          ＋ {{ t("dsplugins.addPlugin") }}
        </button>
        <button class="dp-btn" @click="checkUpdates" :disabled="busy">
          {{ t("dsplugins.checkUpdates") }}
        </button>
      </div>

      <!-- 插件列表 -->
      <div v-if="!currentProfile() || !currentProfile().plugins.length" class="dp-state dp-state-empty">
        <span class="dp-state-icon">🧩</span>
        <span>{{ t("dsplugins.empty") }}</span>
      </div>
      <ul v-else-if="filteredPlugins.length" class="dp-list">
        <li v-for="pl in filteredPlugins" :key="pl.name" class="dp-item">
          <div class="dp-item-main">
            <span class="dp-item-icon">{{ pl.is_bundle ? "📦" : "🧩" }}</span>
            <div class="dp-item-info">
              <code class="dp-item-name" :title="pl.description || ''">{{ pl.name }}</code>
              <span
                class="dp-item-ver"
                v-if="pl.version"
                :title="pl.spec ? t('dsplugins.specTip', { spec: pl.spec }) : ''"
              >v{{ pl.version }}</span>
            </div>
            <span class="dp-badge" :class="isOfficial(pl) ? 'dp-badge-bundle' : 'dp-badge-user'">{{ srcLabel(pl) }}</span>
            <span class="dp-badge dp-badge-kind" v-if="!pl.declares_bundle">{{ kindLabel(pl) }}</span>
            <span class="dp-badge dp-badge-update" v-if="updates[pl.name]">
              {{ t("dsplugins.newVersion", { v: updates[pl.name].latest }) }}
            </span>
          </div>
          <button
            v-if="updates[pl.name]"
            class="dp-btn dp-btn-primary dp-btn-sm"
            @click="updatePlugin(pl)"
            :disabled="busy"
          >
            {{ t("dsplugins.update") }}
          </button>
          <button
            class="dp-btn dp-btn-danger dp-btn-sm"
            @click="doAction('remove', pl.name, isOfficial(pl))"
            :disabled="busy || isOfficial(pl)"
            :title="isOfficial(pl) ? t('dsplugins.bundleLocked') : ''"
          >
            {{ t("dsplugins.remove") }}
          </button>
        </li>
      </ul>
      <div v-else class="dp-state dp-state-empty">{{ t("dsplugins.noMatch") }}</div>

      <!-- B1-08：操作输出（实时进度行） -->
      <div class="dp-log" v-if="logLines.length">
        <button class="dp-log-toggle" @click="showLog = !showLog">
          {{ showLog ? t("dsplugins.hideLog") : t("dsplugins.showLog") }} ({{ logLines.length }})
        </button>
        <pre class="dp-log-body" v-show="showLog">{{ logLines.join("\n") }}</pre>
      </div>

      <!-- B1-11/B1-12：官方信息（待决定构建脚本 / 版本豁免） -->
      <div class="dp-meta">
        <div class="dp-pending" v-if="profileInfo.pending_builds.length">
          <span class="dp-meta-label">{{ t("dsplugins.pendingBuilds", { n: profileInfo.pending_builds.length }) }}</span>
          <code class="dp-meta-code">{{ profileInfo.pending_builds.join(", ") }}</code>
          <button class="dp-btn dp-btn-primary dp-btn-sm" @click="approveBuilds" :disabled="busy">
            {{ t("dsplugins.approveRetry") }}
          </button>
        </div>
        <details class="dp-exemptions">
          <summary>{{ t("dsplugins.exemptions") }} ({{ exemptionCount }})</summary>
          <div v-if="!exemptionCount" class="dp-meta-empty">{{ t("dsplugins.exemptionsEmpty") }}</div>
          <ul v-else class="dp-exemption-list">
            <li v-for="(runtimes, spec) in profileInfo.exemptions" :key="spec" class="dp-exemption">
              <code>{{ spec }}</code>
              <span v-for="rt in runtimes" :key="rt" class="dp-exemption-rt">
                {{ rt }}
                <button class="dp-btn dp-btn-danger dp-btn-sm" @click="revokeExemption(spec, rt)">
                  {{ t("dsplugins.revoke") }}
                </button>
              </span>
            </li>
          </ul>
          <!-- B1-14：允许版本不兼容（官方 allow-version --accept-risk） -->
          <div class="dp-allow">
            <input class="dp-input dp-allow-spec" v-model="allowSpec" :placeholder="t('dsplugins.allowSpecPlaceholder')" />
            <input class="dp-input dp-allow-rt" v-model="allowRuntime" :placeholder="profileInfo.runtime_version || '0.2.1-rc.2'" />
            <label class="dp-allow-risk">
              <input type="checkbox" v-model="allowRisk" />
              {{ t("dsplugins.allowRisk") }}
            </label>
            <button
              class="dp-btn dp-btn-primary dp-btn-sm"
              @click="allowExemption"
              :disabled="!allowSpec.trim() || !allowRisk"
            >
              {{ t("dsplugins.allowExemption") }}
            </button>
          </div>
        </details>
      </div>
    </template>

    <!-- 官方风格安装对话框：form → confirm → installing → done/failed -->
    <div class="dp-modal-mask" v-if="showInstall" @click.self="closeInstall">
      <div class="dp-modal">
        <div class="dp-modal-head">
          <span class="dp-modal-title">{{ t("dsplugins.addPlugin") }}</span>
          <button class="dp-icon-btn" @click="closeInstall">✕</button>
        </div>

        <template v-if="installPhase === 'form'">
          <div class="dp-modal-desc">{{ t("dsplugins.installDesc") }}</div>
          <input
            class="dp-input dp-modal-input"
            v-model="installSpec"
            :placeholder="t('dsplugins.specPlaceholder')"
            :disabled="installing"
            @keyup.enter="requestInstall"
          />
          <details class="dp-guide">
            <summary>{{ t("dsplugins.guideTitle") }}</summary>
            <div class="dp-guide-body">
              {{ t("dsplugins.guideText") }}
              <button class="dp-link-btn" @click="useExample">{{ t("dsplugins.fillExample") }}</button>
            </div>
          </details>
          <div class="dp-field">
            <label>{{ t("dsplugins.registryLabel") }}</label>
            <select class="dp-select" v-model="installRegistry" :disabled="installing">
              <option value="">{{ t("dsplugins.registryDefault") }}</option>
              <option value="official">{{ t("dsplugins.registryOfficial") }}</option>
              <option value="mirror">{{ t("dsplugins.registryMirror") }}</option>
              <option value="custom">{{ t("dsplugins.registryCustom") }}</option>
            </select>
          </div>
          <div class="dp-modal-hint" v-if="registryProbed">{{ t("dsplugins.probeRecommended") }}</div>
          <input
            v-if="installRegistry === 'custom'"
            class="dp-input dp-modal-input"
            v-model="customRegistry"
            :placeholder="t('dsplugins.customRegistryPlaceholder')"
            :disabled="installing"
          />
          <div class="dp-warn">{{ t("dsplugins.trustWarning") }}</div>
          <button
            class="dp-btn dp-btn-primary dp-modal-install"
            @click="requestInstall"
            :disabled="!installSpec.trim()"
          >
            {{ t("dsplugins.install") }}
          </button>
        </template>

        <template v-else-if="installPhase === 'checking'">
          <div class="dp-modal-desc">
            <span class="dp-spinner"></span>
            <span style="margin-left: 6px">{{ t("dsplugins.checking") }}</span>
          </div>
        </template>

        <template v-else-if="installPhase === 'confirm'">
          <div class="dp-modal-desc">{{ t("dsplugins.confirmHint") }}</div>
          <div class="dp-confirm-row">
            <span class="dp-confirm-label">{{ t("dsplugins.specLabel") }}</span>
            <code>{{ inspectInfo?.name || installSpec.trim() }}<span v-if="inspectInfo?.version"> · v{{ inspectInfo.version }}</span></code>
          </div>
          <div class="dp-confirm-row" v-if="inspectInfo?.description">
            <span class="dp-confirm-label">{{ t("dsplugins.descriptionLabel") }}</span>
            <span>{{ inspectInfo.description }}</span>
          </div>
          <div class="dp-confirm-row">
            <span class="dp-confirm-label">{{ t("dsplugins.registryLabel") }}</span>
            <span>{{ registryLabelText }}</span>
          </div>
          <div class="dp-modal-hint" v-if="inspectInfo?.kind === 'git'">
            {{ t("dsplugins.inspectGitNote", { p: inspectInfo.detail || installSpec.trim() }) }}
          </div>
          <div class="dp-warn">{{ t("dsplugins.trustWarning") }}</div>
          <div class="dp-modal-actions">
            <button class="dp-btn" @click="backToForm">{{ t("dsplugins.back") }}</button>
            <button class="dp-btn dp-btn-primary" @click="confirmInstall">
              {{ t("dsplugins.confirmInstall") }}
            </button>
          </div>
        </template>

        <template v-else-if="installPhase === 'installing'">
          <div class="dp-modal-desc">
            <span class="dp-spinner"></span>
            <span style="margin-left: 6px">{{ t("dsplugins.installing") }}</span>
          </div>
          <pre class="dp-log-body dp-modal-log">{{ logLines.slice(-12).join("\n") }}</pre>
          <div class="dp-modal-hint">{{ t("dsplugins.installNoCancelHint") }}</div>
        </template>

        <template v-else-if="installPhase === 'done'">
          <div class="dp-done-title">✓ {{ t("dsplugins.installDoneTitle") }}</div>
          <div class="dp-modal-desc">
            {{ t("dsplugins.installed", { p: completedPlugin?.name || installSpec.trim() }) }}
            <span v-if="completedPlugin"> · v{{ completedPlugin.version }}</span>
          </div>
          <div class="dp-modal-hint">{{ t("dsplugins.installDoneHint") }}</div>
          <div class="dp-modal-actions">
            <button class="dp-btn dp-btn-primary" @click="closeInstall">{{ t("dsplugins.done") }}</button>
          </div>
        </template>

        <template v-else>
          <div class="dp-done-title dp-fail-title">✕ {{ t("dsplugins.installFailedTitle") }}</div>
          <pre class="dp-log-body dp-modal-log">{{ installError }}</pre>
          <div class="dp-pending" v-if="profileInfo.pending_builds.length">
            <span class="dp-meta-label">{{ t("dsplugins.pendingBuilds", { n: profileInfo.pending_builds.length }) }}</span>
            <code class="dp-meta-code">{{ profileInfo.pending_builds.join(", ") }}</code>
            <button class="dp-btn dp-btn-primary dp-btn-sm" @click="approveAndRetryInstall">
              {{ t("dsplugins.approveRetry") }}
            </button>
          </div>
          <div class="dp-modal-actions">
            <button class="dp-btn" @click="backToForm">{{ t("dsplugins.changeSource") }}</button>
            <button class="dp-btn dp-btn-primary" @click="retryInstall">{{ t("dsplugins.retry") }}</button>
            <button class="dp-btn" @click="closeInstall">{{ t("dsplugins.close") }}</button>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ds-plugin-panel {
  flex: 1 0 100%;
  width: 100%;
  box-sizing: border-box;
  margin: 10px 0 4px 0;
  padding: 0;
  background: var(--panel-bg);
  border: 1px solid var(--panel-border);
  border-radius: 10px;
  overflow: hidden;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);
}

/* 头部 */
.dp-head {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 12px 16px;
  background: linear-gradient(180deg, var(--surface-2), var(--panel-bg));
  border-bottom: 1px solid var(--panel-border);
}
.dp-head-left { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; }
.dp-title { font-size: 13px; font-weight: 600; color: var(--text); }
.dp-home {
  display: inline-flex; align-items: center; gap: 6px;
  font-size: 11px; padding: 2px 8px; border-radius: 12px;
  border: 1px solid var(--panel-border); background: var(--surface-3);
  max-width: 360px; overflow: hidden;
}
.dp-home-kind { font-weight: 600; white-space: nowrap; }
.dp-home-path {
  color: var(--text-muted); font-size: 10.5px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.dp-home.kind-official { border-color: #f5c542; background: rgba(245,197,66,0.12); }
.dp-home.kind-official .dp-home-kind { color: #b8860b; }
.dp-home.kind-isolated { border-color: #30a46c; background: rgba(48,164,108,0.1); }
.dp-home.kind-isolated .dp-home-kind { color: #30a46c; }
.dp-home.kind-shared .dp-home-kind { color: var(--text-3); }

.dp-head-actions { display: flex; gap: 4px; flex: none; }
.dp-icon-btn {
  width: 26px; height: 26px; border-radius: 6px; cursor: pointer;
  border: 1px solid var(--panel-border); background: var(--panel-bg);
  color: var(--text-3); font-size: 13px; line-height: 1;
  display: inline-flex; align-items: center; justify-content: center;
  transition: all .15s;
}
.dp-icon-btn:hover:not(:disabled) { background: var(--surface-3); color: var(--text); border-color: var(--accent-border); }
.dp-icon-btn:disabled { opacity: .4; cursor: not-allowed; }

.dp-hint {
  padding: 8px 16px; font-size: 11px; color: var(--text-muted);
  border-bottom: 1px dashed var(--panel-border);
  background: var(--surface-2);
}

/* 状态（加载/空） */
.dp-state {
  display: flex; align-items: center; justify-content: center; gap: 8px;
  padding: 24px 16px; font-size: 12px; color: var(--text-muted);
}
.dp-state-icon { font-size: 18px; opacity: .6; }
.dp-spinner {
  width: 14px; height: 14px; border-radius: 50%;
  border: 2px solid var(--surface-3); border-top-color: var(--accent);
  animation: dp-spin .7s linear infinite;
}
@keyframes dp-spin { to { transform: rotate(360deg); } }

/* 工具栏 */
.dp-toolbar {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  padding: 10px 16px;
}
.dp-select, .dp-input {
  font-size: 12px; padding: 5px 10px; border-radius: 6px;
  border: 1px solid var(--panel-border); background: var(--panel-bg);
  color: var(--text); outline: none;
}
.dp-select:focus, .dp-input:focus { border-color: var(--accent-border); }
.dp-profile-tag {
  font-size: 11px; font-weight: 600; padding: 3px 10px; border-radius: 12px;
  background: var(--accent-weak); color: var(--accent); border: 1px solid var(--accent-border);
}
.dp-add { display: flex; gap: 6px; flex: 1; min-width: 240px; }
.dp-add .dp-input { flex: 1; }

/* 按钮 */
.dp-btn {
  font-size: 12px; padding: 5px 14px; border-radius: 6px; cursor: pointer;
  border: 1px solid var(--panel-border); background: var(--panel-bg); color: var(--text);
  transition: all .15s; white-space: nowrap;
}
.dp-btn:hover:not(:disabled) { background: var(--surface-3); }
.dp-btn:disabled { opacity: .5; cursor: not-allowed; }
.dp-btn-primary {
  background: var(--accent); color: #fff; border-color: var(--accent);
}
.dp-btn-primary:hover:not(:disabled) { filter: brightness(1.08); }
.dp-btn-danger { color: var(--danger, #e5484d); border-color: var(--danger, #e5484d); }
.dp-btn-danger:hover:not(:disabled) { background: rgba(229,72,77,0.08); }
.dp-btn-sm { font-size: 11px; padding: 3px 10px; }

/* 插件列表：自适应列数（宽屏多列，窄屏单列） */
.dp-list {
  list-style: none; margin: 0; padding: 6px 16px 14px;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 8px;
}
.dp-item {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 8px 12px; border-radius: 7px;
  background: var(--surface-2); border: 1px solid transparent;
  transition: all .12s;
  min-width: 0;
}
.dp-item:hover { border-color: var(--panel-border); background: var(--surface-3); }
.dp-item-main { display: flex; align-items: center; gap: 8px; min-width: 0; flex: 1; }
.dp-item-icon { font-size: 14px; flex: none; opacity: .8; }
.dp-item-info { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
.dp-item-name {
  font-size: 12px; color: var(--text); font-family: ui-monospace, Consolas, monospace;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.dp-item-ver { font-size: 10.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; flex: none; }
.dp-badge {
  font-size: 9.5px; padding: 1px 7px; border-radius: 8px; flex: none;
  background: var(--accent-weak); color: var(--accent); border: 1px solid var(--accent-border);
  font-weight: 500;
}
/* 来源：随 DSH 提供 / 已安装（与官方插件页语义对齐） */
.dp-badge-bundle { background: var(--surface-3); color: var(--text-3); border-color: var(--panel-border); }
.dp-badge-user { background: var(--accent-weak); color: var(--accent); border-color: var(--accent-border); }

/* B1-07：无 profile 时的初始化表单 */
.dp-init { flex-direction: column; gap: 10px; }
.dp-init-text { font-size: 12px; }
.dp-init .dp-add { flex: none; width: 100%; max-width: 420px; }

/* B1-10：搜索框 */
.dp-search { min-width: 140px; max-width: 220px; flex: none; }

/* B1-11/B1-12：官方信息（构建脚本 / 版本豁免） */
.dp-meta { border-top: 1px dashed var(--panel-border); padding: 8px 16px 12px; display: flex; flex-direction: column; gap: 8px; }
.dp-pending { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 11.5px; }
.dp-meta-label { color: var(--danger, #e5484d); font-weight: 600; }
.dp-meta-code { color: var(--text-muted); font-size: 11px; font-family: ui-monospace, Consolas, monospace; word-break: break-all; }
.dp-meta-empty { color: var(--text-muted); font-size: 11.5px; padding: 2px 0 0; }
.dp-exemptions summary { cursor: pointer; font-size: 11.5px; color: var(--text-muted); }
.dp-exemption-list { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
.dp-exemption { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 11px; color: var(--text-muted); }
.dp-exemption code { color: var(--text); font-family: ui-monospace, Consolas, monospace; }
.dp-exemption-rt { display: inline-flex; align-items: center; gap: 4px; }
.dp-badge-kind { background: var(--surface-3); color: var(--text-3); border-color: var(--panel-border); }

/* B1-08：操作输出 */
.dp-log { border-top: 1px dashed var(--panel-border); padding: 6px 16px 10px; }
.dp-log-toggle {
  font-size: 11px; color: var(--text-muted); background: none; border: none;
  cursor: pointer; padding: 2px 0;
}
.dp-log-toggle:hover { color: var(--text); }
.dp-log-body {
  margin: 6px 0 0; padding: 8px 10px; max-height: 180px; overflow: auto;
  background: var(--surface-2); border: 1px solid var(--panel-border); border-radius: 6px;
  font-size: 11px; line-height: 1.5; color: var(--text-3);
  font-family: ui-monospace, Consolas, monospace; white-space: pre-wrap; word-break: break-all;
}

/* 官方风格安装对话框 */
.dp-modal-mask {
  position: fixed; inset: 0; z-index: 1000;
  background: rgba(0, 0, 0, 0.35);
  display: flex; align-items: center; justify-content: center;
}
.dp-modal {
  width: min(560px, calc(100vw - 40px));
  max-height: calc(100vh - 40px); overflow: auto;
  background: var(--panel-bg); border: 1px solid var(--panel-border); border-radius: 12px;
  padding: 16px 18px; box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
  display: flex; flex-direction: column; gap: 10px;
}
.dp-modal-head { display: flex; align-items: center; justify-content: space-between; }
.dp-modal-title { font-size: 14px; font-weight: 600; color: var(--text); }
.dp-modal-desc { font-size: 12px; color: var(--text-muted); }
.dp-modal-input { width: 100%; box-sizing: border-box; }
.dp-guide summary { font-size: 12px; color: var(--text-muted); cursor: pointer; }
.dp-guide-body {
  margin-top: 6px; padding: 8px 10px; font-size: 11.5px; line-height: 1.6;
  color: var(--text-muted); background: var(--surface-2);
  border: 1px solid var(--panel-border); border-radius: 6px;
}
.dp-field { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text); }
.dp-field select { flex: 1; }
.dp-warn {
  font-size: 11.5px; line-height: 1.6; color: #a17a08;
  background: rgba(245, 197, 66, 0.14); border: 1px solid rgba(245, 197, 66, 0.4);
  border-radius: 6px; padding: 8px 10px;
}
.dp-modal-install { width: 100%; }
.dp-badge-update {
  background: rgba(229, 72, 77, 0.1); color: var(--danger, #e5484d);
  border-color: rgba(229, 72, 77, 0.4);
}
.dp-allow { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-top: 8px; }
.dp-allow .dp-input { font-size: 11px; padding: 4px 8px; }
.dp-allow-spec { width: 180px; }
.dp-allow-rt { width: 130px; }
.dp-allow-risk { font-size: 11px; color: var(--text-muted); display: inline-flex; align-items: center; gap: 4px; }

/* 安装对话框阶段 */
.dp-confirm-row { display: flex; align-items: baseline; gap: 8px; font-size: 12px; word-break: break-all; }
.dp-confirm-label { color: var(--text-muted); flex: none; }
.dp-confirm-row code { font-family: ui-monospace, Consolas, monospace; color: var(--text); }
.dp-modal-actions { display: flex; justify-content: flex-end; gap: 8px; }
.dp-done-title { font-size: 14px; font-weight: 600; color: #2f9e63; }
.dp-fail-title { color: var(--danger, #e5484d); }
.dp-modal-log { max-height: 160px; margin: 0; }
.dp-modal-hint { font-size: 11.5px; color: var(--text-muted); line-height: 1.6; }
.dp-link-btn {
  display: inline-block; margin-left: 6px; padding: 0; border: none; background: none;
  color: var(--accent); font-size: 11.5px; cursor: pointer; text-decoration: underline;
}
</style>
