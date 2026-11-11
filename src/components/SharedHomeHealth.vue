<script setup>
import { onMounted, ref } from "vue";
import { sharedHomeHealth } from "../api/plugins.js";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";
import { withTimeout } from "../composables/useAsyncTimeout.js";

const props = defineProps({
  version: { type: String, required: true },
});
const emit = defineEmits(["close"]);
const { notify } = useToast();
const loading = ref(false);
const report = ref(null);
const error = ref("");

async function run() {
  loading.value = true;
  error.value = "";
  try {
    report.value = await withTimeout(
      sharedHomeHealth(props.version),
      15000,
      t("dsplugins.healthTimeout"),
    );
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function copyReport() {
  if (!report.value) return;
  try {
    await navigator.clipboard.writeText(JSON.stringify(report.value, null, 2));
    notify(t("dsplugins.healthCopied"));
  } catch (_) {
    notify(t("dsplugins.healthCopyFailed"));
  }
}

onMounted(run);
</script>

<template>
  <div class="dp-modal-mask" @click.self="emit('close')">
    <div class="dp-modal dp-health-modal">
      <div class="dp-modal-head">
        <span class="dp-modal-title">{{ t("dsplugins.healthTitle") }}</span>
        <button class="dp-icon-btn" @click="emit('close')">✕</button>
      </div>
      <div class="dp-modal-desc">{{ t("dsplugins.healthDesc") }}</div>
      <div v-if="loading" class="dp-modal-desc">
        <span class="dp-spinner"></span>
        <span style="margin-left: 6px">{{ t("dsplugins.healthScanning") }}</span>
      </div>
      <pre v-else-if="error" class="dp-log-body dp-modal-log">{{ error }}</pre>
      <template v-else-if="report">
        <div class="dp-health-summary">
          <span>{{ t("dsplugins.healthScanned", { n: report.summary.scanned }) }}</span>
          <span>{{ t("dsplugins.healthStale", { n: report.summary.stale }) }}</span>
          <span>{{ t("dsplugins.healthDangling", { n: report.summary.dangling }) }}</span>
          <span>{{ t("dsplugins.healthDuplicate", { n: report.summary.duplicate }) }}</span>
        </div>
        <div class="dp-health-home">{{ report.home }}</div>
        <div v-if="!report.issues.length" class="dp-modal-hint">{{ t("dsplugins.healthNoIssues") }}</div>
        <div v-else class="dp-health-list">
          <div class="dp-health-issue" v-for="(issue, i) in report.issues" :key="i">
            <div class="dp-health-issue-head">
              <code>{{ issue.package }}</code>
              <span class="dp-badge">{{ issue.kind }}</span>
            </div>
            <div class="dp-modal-hint">{{ issue.detail }}</div>
            <code v-if="issue.target" class="dp-meta-code">{{ issue.target }}</code>
          </div>
        </div>
        <div class="dp-modal-hint">{{ t("dsplugins.healthRecommendation") }}</div>
      </template>
      <div class="dp-modal-actions">
        <button class="dp-btn" @click="copyReport" :disabled="!report">{{ t("dsplugins.healthCopy") }}</button>
        <button class="dp-btn" @click="run" :disabled="loading">{{ t("dsplugins.healthRetry") }}</button>
        <button class="dp-btn dp-btn-primary" @click="emit('close')">{{ t("dsplugins.close") }}</button>
      </div>
    </div>
  </div>
</template>
