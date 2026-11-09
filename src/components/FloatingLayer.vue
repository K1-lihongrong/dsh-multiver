<script setup>
import { t } from "../composables/useI18n.js";
import RuntimeGuide from "./RuntimeGuide.vue";

defineProps({
  toast: { type: String, default: "" },
  toastExpanded: { type: Boolean, default: false },
  guideOpen: { type: Boolean, default: false },
});
defineEmits(["toggle-toast", "dismiss-toast", "close-guide", "open-url"]);
</script>

<template>
  <transition name="fade">
    <div
      class="toast"
      :class="{ expanded: toastExpanded, long: String(toast).length > 60 }"
      v-if="toast"
      @click="$emit('toggle-toast')"
    >
      <div class="toast-text">{{ toast }}</div>
      <div class="toast-hint" v-if="String(toast).length > 60 && !toastExpanded">
        {{ t("app.toastHint") }}
      </div>
      <button class="toast-close" @click.stop="$emit('dismiss-toast')">×</button>
    </div>
  </transition>

  <RuntimeGuide v-if="guideOpen" @close="$emit('close-guide')" @open-url="$emit('open-url', $event)" />
</template>

<style>
.toast {
  position: fixed; bottom: 22px; left: 50%; transform: translateX(-50%);
  background: var(--toast-bg); color: var(--toast-text); padding: 12px 40px 12px 18px;
  border-radius: 9px; font-size: 13px; max-width: 80%;
  box-shadow: 0 6px 24px var(--shadow-pop2); z-index: 100;
  cursor: pointer; user-select: text;
}
.toast.long { max-height: 60vh; overflow-y: auto; }
.toast.expanded { max-height: 60vh; overflow-y: auto; }
.toast-text {
  white-space: pre-wrap; word-break: break-all; line-height: 1.5;
  max-height: 4.5em; overflow: hidden;
}
.toast.expanded .toast-text { max-height: none; }
.toast-hint {
  margin-top: 6px; font-size: 11px; color: var(--text-muted);
}
.toast-close {
  position: absolute; top: 8px; right: 10px;
  background: none; border: none; color: var(--text-muted);
  font-size: 18px; line-height: 1; cursor: pointer; padding: 0 4px;
  transition: color var(--motion-fast) var(--ease-standard);
}
.toast-close:hover { color: var(--toast-text); }
.toast-close:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.fade-enter-active, .fade-leave-active { transition: opacity .25s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
