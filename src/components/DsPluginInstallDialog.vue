<script setup>
import { ref, computed, inject, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";
import { actionErrorText, parseIncompatiblePlugins } from "../composables/useDsPluginErrors.js";
import { withTimeout } from "../composables/useAsyncTimeout.js";
import { classifyPluginFailure } from "../api/plugins.js";
import { DS_PLUGIN_LOG_KEY } from "../composables/useDsPluginLog.js";

// 官方风格安装对话框：form → checking → confirm → installing → done/failed。
// 从 DsPluginPanel 拆出（v0.3.3 D21）。阶段状态、安装源记忆都在本组件内，
// 安装成功后通知壳重新拉列表 + 刷新官方信息。
const props = defineProps({
  version: { type: String, required: true },
  profile: { type: String, default: "" },
  profileInfo: { type: Object, required: true },
  plugins: { type: Array, default: () => [] },
});
const emit = defineEmits(["close", "installed", "refresh-info", "installing-change"]);

const { notify } = useToast();
const { logLines } = inject(DS_PLUGIN_LOG_KEY);

const show = ref(false);
const installSpec = ref("");
const installRegistry = ref(""); // ""=默认（pnpm 配置）, official, mirror, custom
const customRegistry = ref("");
const installing = ref(false);

const installPhase = ref("form");
const installResult = ref(null);
const installError = ref("");
const incompatible = ref([]);
const failure = ref(null);
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
  return props.plugins.find((p) => p.name === name) || null;
});

/// 打开对话框。prefill 有值时预填 spec（推荐页的「安装」按钮走这条路）。
async function open(prefill) {
  show.value = true;
  installSpec.value = prefill || "";
  installPhase.value = "form";
  installResult.value = null;
  installError.value = "";
  incompatible.value = [];
  failure.value = null;
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
      profile: props.profile,
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

function exemptionCommand(item) {
  return (
    "dsh plugin --profile " + props.profile + " allow-version " +
    item.packageVersion + " --dsh-version " + item.dshVersion + " --accept-risk"
  );
}

async function allowIncompatibleAndRetry(onlyItem) {
  const items = onlyItem ? [onlyItem] : incompatible.value.slice();
  if (!items.length) return;
  try {
    for (const item of items) {
      await withTimeout(
        invoke("ds_exemption_action", {
          version: props.version,
          profile: props.profile,
          action: "allow",
          package_version: item.packageVersion,
          dsh_version: item.dshVersion,
        }),
        60000,
        t("dsplugins.operationTimeout"),
      );
    }
    notify(t("dsplugins.exemptionAllowed", { n: items.length }));
    emit("refresh-info");
    incompatible.value = incompatible.value.filter((x) => !items.includes(x));
    await confirmInstall();
  } catch (e) {
    notify("" + e);
  }
}

// 失败界面内直接允许 pnpm 拦截的构建脚本并重试安装（对齐官方）
async function approveAndRetryInstall() {
  const pkgs = props.profileInfo.pending_builds || [];
  if (!pkgs.length) return;
  try {
    await invoke("ds_approve_builds", {
      version: props.version,
      profile: props.profile,
      packages: pkgs,
    });
    emit("refresh-info");
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
    const res = await withTimeout(
      invoke("ds_plugin_action", {
        version: props.version,
        profile: props.profile,
        action: "add",
        package: spec,
        registry: registryValue.value,
      }),
      300000,
      t("dsplugins.operationTimeout"),
    );
    installResult.value = { name: res?.name || null, spec, registry: registryValue.value };
    installPhase.value = "done";
    emit("installed");
    emit("refresh-info");
  } catch (e) {
    const raw = String(e) + "\n" + logLines.value.join("\n");
    installError.value = actionErrorText(e);
    incompatible.value = parseIncompatiblePlugins(raw);
    try {
      failure.value = await classifyPluginFailure(raw);
    } catch (_) {
      failure.value = null;
    }
    installPhase.value = "failed";
    emit("refresh-info");
  } finally {
    installing.value = false;
  }
}

async function copyFailureRaw() {
  if (!failure.value) return;
  try {
    await navigator.clipboard.writeText(failure.value.raw);
    notify(t("dsplugins.healthCopied"));
  } catch (e) {
    notify("" + e);
  }
}

function backToForm() {
  installPhase.value = "form";
}

// 安装中状态对推荐页可见（安装按钮要禁用）
watch(installing, (on) => emit("installing-change", on));

function closeInstall() {
  show.value = false;
  emit("close");
}

function useExample() {
  installSpec.value = t("dsplugins.exampleSpec");
}

defineExpose({ open });
</script>

<template>
  <!-- 官方风格安装对话框：form → confirm → installing → done/failed -->
  <div class="dp-modal-mask" v-if="show" @click.self="closeInstall">
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
        <div class="dp-failure-card" v-if="failure">
          <strong>{{ t("dsplugins.failure." + failure.code) }}</strong>
          <span class="dp-modal-hint">{{ failure.detail }}</span>
          <button class="dp-link-btn" @click="copyFailureRaw">
            {{ t("dsplugins.copyRaw") }}
          </button>
        </div>
        <pre class="dp-log-body dp-modal-log">{{ installError }}</pre>
        <div class="dp-pending" v-if="profileInfo.pending_builds.length">
          <span class="dp-meta-label">{{ t("dsplugins.pendingBuilds", { n: profileInfo.pending_builds.length }) }}</span>
          <code class="dp-meta-code">{{ profileInfo.pending_builds.join(", ") }}</code>
          <button class="dp-btn dp-btn-primary dp-btn-sm" @click="approveAndRetryInstall">
            {{ t("dsplugins.approveRetry") }}
          </button>
        </div>
        <div class="dp-incompatible" v-if="incompatible.length">
          <div class="dp-meta-label">{{ t("dsplugins.incompatibleFound", { n: incompatible.length }) }}</div>
          <div class="dp-incompatible-row" v-for="item in incompatible" :key="item.packageVersion + item.dshVersion">
            <code class="dp-meta-code">{{ item.packageVersion }} · DSH {{ item.dshVersion }}</code>
            <button class="dp-btn dp-btn-sm" @click="allowIncompatibleAndRetry(item)">
              {{ t("dsplugins.allowExactVersion") }}
            </button>
          </div>
          <div class="dp-modal-hint">{{ t("dsplugins.exemptionCommand") }}</div>
          <code class="dp-meta-code">{{ exemptionCommand(incompatible[0]) }}</code>
          <button class="dp-btn dp-btn-primary dp-btn-sm" @click="allowIncompatibleAndRetry()">
            {{ t("dsplugins.allowExactVersionsRetry", { n: incompatible.length }) }}
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
</template>
