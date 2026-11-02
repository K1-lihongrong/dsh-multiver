<script setup>
import { invoke } from "@tauri-apps/api/core";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";

// B1-11/B1-12：profile 级官方信息——待决定的构建脚本。
// 从 DsPluginPanel 拆出（v0.3.3 D21）。数据由壳持有并下发，本组件只发命令 + 通知刷新。
//
// 版本豁免区块已于 v0.3.3 移除：dsh CLI 至今没有 allow-version / revoke-version
// 子命令（dsh plugin 只是把参数转发给 pnpm），豁免机制只存在于 dsh-app-boot 内部，
// 没有可调用的入口。等 dsh 提供 CLI 后按 docs/开发缺口.md 的 Backlog 恢复。
const props = defineProps({
  version: { type: String, required: true },
  profile: { type: String, default: "" },
  profileInfo: { type: Object, required: true },
  busy: { type: Boolean, default: false },
});
const emit = defineEmits(["approved"]);

const { notify } = useToast();

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
</script>

<template>
  <div class="dp-meta" v-if="profileInfo.pending_builds.length">
    <div class="dp-pending">
      <span class="dp-meta-label">{{ t("dsplugins.pendingBuilds", { n: profileInfo.pending_builds.length }) }}</span>
      <code class="dp-meta-code">{{ profileInfo.pending_builds.join(", ") }}</code>
      <button class="dp-btn dp-btn-primary dp-btn-sm" @click="approveBuilds" :disabled="busy">
        {{ t("dsplugins.approveRetry") }}
      </button>
    </div>
  </div>
</template>
