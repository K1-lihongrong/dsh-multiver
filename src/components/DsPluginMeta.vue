<script setup>
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";

// B1-11/B1-12：profile 级官方信息——待决定的构建脚本、版本豁免。
// 从 DsPluginPanel 拆出（v0.3.3 D21）。数据由壳持有并下发，本组件只发命令 + 通知刷新。
const props = defineProps({
  version: { type: String, required: true },
  profile: { type: String, default: "" },
  profileInfo: { type: Object, required: true },
  busy: { type: Boolean, default: false },
});
const emit = defineEmits(["refresh-info", "approved"]);

const { notify } = useToast();

const allowSpec = ref("");
const allowRuntime = ref("");
const allowRisk = ref(false);

const exemptionCount = computed(() => Object.keys(props.profileInfo.exemptions || {}).length);

// B1-12：允许 pnpm 拦截的依赖构建脚本（写 pnpm-workspace.yaml 的 allowBuilds），成功后由壳重试上次操作
async function approveBuilds() {
  const pkgs = props.profileInfo.pending_builds || [];
  if (!pkgs.length) return;
  try {
    const left = await invoke("ds_approve_builds", {
      version: props.version,
      profile: props.profile,
      packages: pkgs,
    });
    props.profileInfo.pending_builds = left || [];
    notify(t("dsplugins.buildsApproved", { n: pkgs.length }));
    emit("approved");
  } catch (e) {
    notify("" + e);
  }
}

// B1-14：允许版本不兼容（官方 allow-version --accept-risk）
async function allowExemption() {
  const spec = allowSpec.value.trim();
  const runtime = (allowRuntime.value || props.profileInfo.runtime_version || "").trim();
  if (!spec || !runtime || !allowRisk.value) return;
  try {
    await invoke("ds_exemption_action", {
      version: props.version,
      profile: props.profile,
      action: "allow",
      package_version: spec,
      dsh_version: runtime,
    });
    notify(t("dsplugins.exemptionAllowed", { p: spec }));
    allowSpec.value = "";
    allowRisk.value = false;
    emit("refresh-info");
  } catch (e) {
    notify("" + e);
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
      profile: props.profile,
      action: "revoke",
      package_version: spec,
      dsh_version: runtime,
    });
    notify(t("dsplugins.exemptionRevoked", { p: spec }));
    emit("refresh-info");
  } catch (e) {
    notify("" + e);
  }
}
</script>

<template>
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
