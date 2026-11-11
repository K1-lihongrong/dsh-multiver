<script setup>
import { t } from "../composables/useI18n.js";

defineProps({
  rootWritable: { type: Boolean, default: true },
  rootWritableError: { type: String, default: "" },
  rootDir: { type: String, default: "" },
  updateError: { type: String, default: "" },
  updateDismissed: { type: Boolean, default: false },
  updateInfo: { type: Object, default: null },
  updating: { type: Boolean, default: false },
  runtimeNeeds: { type: Boolean, default: false },
  runtimeDismissed: { type: Boolean, default: false },
});
defineEmits([
  "go-root-settings",
  "retry-check",
  "dismiss-update-error",
  "open-url",
  "one-click-update",
  "dismiss-update",
  "prepare-runtime",
  "open-guide",
  "dismiss-runtime",
]);
</script>

<template>
  <div class="status-banners">
    <!-- D19：数据根不可写。持续状态 → 不可关闭；根目录改好后 state 刷新即自动消失 -->
    <div class="update-banner root-unwritable-banner" v-if="!rootWritable">
      <span class="update-text">
        {{ t("root.unwritable", { path: rootDir, err: rootWritableError }) }}
      </span>
      <span class="update-actions">
        <button class="btn small primary" @click="$emit('go-root-settings')">{{ t("root.goSettings") }}</button>
      </span>
    </div>

    <div class="update-banner update-banner-error" v-if="updateError && !updateDismissed">
      <span class="update-text">{{ t("update.checkFailed", { err: updateError }) }}</span>
      <span class="update-actions">
        <button class="btn small" @click="$emit('retry-check')">{{ t("update.retryCheck") }}</button>
        <button class="btn small" @click="$emit('dismiss-update-error')">×</button>
      </span>
    </div>

    <div class="update-banner" v-if="updateInfo && !updateDismissed">
      <span class="update-text">
        {{ t("update.available", { latest: updateInfo.latest, current: updateInfo.current }) }}
      </span>
      <span class="update-actions">
        <button
          v-if="updateInfo.download_url && !updating"
          class="btn small primary"
          @click="$emit('one-click-update')"
        >{{ t("update.updateNow") }}</button>
        <span v-if="updating" class="update-progress">{{ t("update.updating") }}</span>
        <button class="btn small" v-if="updateInfo.release_url" @click="$emit('open-url', updateInfo.release_url)">
          {{ t("update.viewRelease") }}
        </button>
        <button class="btn small" @click="$emit('dismiss-update')">×</button>
      </span>
    </div>

    <div class="runtime-banner" v-if="runtimeNeeds && !runtimeDismissed">
      <span class="runtime-text">{{ t("runtime.banner") }}</span>
      <span class="runtime-actions">
        <button class="btn small primary" @click="$emit('prepare-runtime')">{{ t("runtime.prepare") }}</button>
        <button class="btn small" @click="$emit('open-guide')">{{ t("runtime.guide") }}</button>
        <button class="btn small" @click="$emit('dismiss-runtime')">{{ t("runtime.ignore") }}</button>
      </span>
    </div>
  </div>
</template>

<style>
.status-banners { display: flex; flex-direction: column; gap: 16px; }
.update-banner {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 10px 14px; border-radius: 9px;
  background: var(--accent-weak); border: 1px solid var(--accent-border);
  font-size: 13px; flex-wrap: wrap;
}
.update-text { color: var(--text); }
.update-actions { display: flex; gap: 8px; align-items: center; }
.update-banner-error { border-color: var(--danger, #e5484d); background: rgba(229, 72, 77, 0.08); }
.root-unwritable-banner { border-color: var(--danger, #e5484d); background: rgba(229, 72, 77, 0.08); }
.root-unwritable-banner .update-text { color: var(--danger, #e5484d); }
.update-progress { font-size: 12px; color: var(--text-3); }

.runtime-banner {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 10px 14px; border-radius: 9px;
  background: var(--warn-weak, var(--accent-weak)); border: 1px solid var(--warn-border, var(--accent-border));
  font-size: 13px; flex-wrap: wrap;
}
.runtime-text { color: var(--text); line-height: 1.5; }
.runtime-actions { display: flex; gap: 8px; align-items: center; flex-shrink: 0; }
</style>
