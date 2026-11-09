<script setup>
import { computed } from "vue";
import { dsExemptionAction, dsApproveBuilds } from "../api/plugins.js";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";

// B1-11/B1-12：profile 级官方信息——待决定的构建脚本。
// 从 DsPluginPanel 拆出（v0.3.3 D21）。数据由壳持有并下发，本组件只发命令 + 通知刷新。
//
// 版本豁免：dsh 0.2.2-rc.1 起，dsh 自己拦截 allow-version / revoke-version /
// version-exemptions 三个子命令，不再转发给 pnpm；这里展示并撤销已有豁免。
const props = defineProps({
  version: { type: String, required: true },
  profile: { type: String, default: "" },
  profileInfo: { type: Object, required: true },
  busy: { type: Boolean, default: false },
});
const emit = defineEmits(["approved", "refresh"]);

const { notify } = useToast();

const exemptionItems = computed(() => {
  const out = [];
  const data = props.profileInfo.exemptions || {};
  for (const [pkgVersion, versions] of Object.entries(data)) {
    for (const dshVersion of versions || []) {
      out.push({ pkgVersion, dshVersion });
    }
  }
  return out;
});

async function revokeExemption(item) {
  try {
    await dsExemptionAction(props.version, props.profile, "revoke", item.pkgVersion, item.dshVersion);
    notify(t("dsplugins.exemptionRevoked", { p: item.pkgVersion, v: item.dshVersion }));
    emit("refresh");
  } catch (e) {
    notify("" + e);
  }
}

// B1-12：允许 pnpm 拦截的依赖构建脚本（写 pnpm-workspace.yaml 的 allowBuilds），成功后由壳重试上次操作
async function approveBuilds() {
  const pkgs = props.profileInfo.pending_builds || [];
  if (!pkgs.length) return;
  try {
    const left = await dsApproveBuilds(props.version, props.profile, pkgs);
    props.profileInfo.pending_builds = left || [];
    notify(t("dsplugins.buildsApproved", { n: pkgs.length }));
    emit("approved");
  } catch (e) {
    notify("" + e);
  }
}
</script>

<template>
  <div class="dp-meta" v-if="profileInfo.pending_builds.length || exemptionItems.length">
    <div class="dp-pending">
      <span class="dp-meta-label">{{ t("dsplugins.pendingBuilds", { n: profileInfo.pending_builds.length }) }}</span>
      <code class="dp-meta-code">{{ profileInfo.pending_builds.join(", ") }}</code>
      <button class="dp-btn dp-btn-primary dp-btn-sm" @click="approveBuilds" :disabled="busy">
        {{ t("dsplugins.approveRetry") }}
      </button>
    </div>
    <div class="dp-exemptions" v-if="exemptionItems.length">
      <div class="dp-meta-label">{{ t("dsplugins.existingExemptions", { n: exemptionItems.length }) }}</div>
      <div class="dp-incompatible-row" v-for="item in exemptionItems" :key="item.pkgVersion + item.dshVersion">
        <code class="dp-meta-code">{{ item.pkgVersion }} · DSH {{ item.dshVersion }}</code>
        <button class="dp-btn dp-btn-sm" @click="revokeExemption(item)" :disabled="busy">
          {{ t("dsplugins.revokeExemption") }}
        </button>
      </div>
    </div>
  </div>
</template>
