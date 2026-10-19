<script setup>
import { t } from "../composables/useI18n.js";

// v0.2.14：运行"共享 home"版本前的确认对话框。
// 三个选择：继续 / 我知道了（继续 + 以后不再提示）/ 取消。
const props = defineProps({
  version: { type: String, required: true },
});
const emit = defineEmits(["continue", "continue-dont-ask", "cancel"]);
</script>

<template>
  <div class="modal-mask" @click.self="emit('cancel')">
    <div class="modal">
      <h3>{{ t("installed.sharedHomeTitle") }}</h3>
      <div class="modal-desc">{{ t("installed.sharedHomeConfirm", { v: props.version }) }}</div>
      <div class="modal-actions">
        <button class="btn" @click="emit('cancel')">{{ t("installed.sharedHomeCancel") }}</button>
        <button class="btn" @click="emit('continue-dont-ask')">{{ t("installed.sharedHomeGotIt") }}</button>
        <button class="btn primary" @click="emit('continue')">{{ t("installed.sharedHomeContinue") }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-mask {
  position: fixed; inset: 0; z-index: 1000;
  background: var(--shadow-strong);
  display: flex; align-items: center; justify-content: center;
}
.modal {
  width: 440px; max-width: 92vw; max-height: 86vh; overflow: auto;
  background: var(--panel-bg); border: 1px solid var(--panel-border);
  border-radius: 12px; padding: 20px 22px;
  box-shadow: 0 12px 40px var(--shadow-pop2);
}
.modal h3 { margin: 0 0 12px; font-size: 15px; }
.modal-desc { font-size: 13px; color: var(--text); line-height: 1.6; white-space: pre-wrap; }
.modal-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 20px; }
</style>
