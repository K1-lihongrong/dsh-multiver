<script setup>
import { t } from "../composables/useI18n.js";

// 运行时「正式安装」综合说明对话框（v0.2.9）。
// 用于告知用户：便携版装在数据根（不污染系统）；想全局安装可去官网。
const emit = defineEmits(["close", "open-url"]);

const NODE_URL = "https://nodejs.org/zh-cn/download";
const PNPM_URL = "https://pnpm.io/zh/installation";
</script>

<template>
  <div class="modal-mask" @click.self="emit('close')">
    <div class="modal">
      <h3>{{ t("runtimeGuide.title") }}</h3>
      <div class="modal-desc">{{ t("runtimeGuide.intro") }}</div>

      <div class="guide-block">
        <div class="guide-name">Node.js</div>
        <div class="guide-desc">{{ t("runtimeGuide.nodeDesc") }}</div>
        <button class="btn small primary" @click="emit('open-url', NODE_URL)">
          {{ t("runtimeGuide.openNode") }}
        </button>
      </div>

      <div class="guide-block">
        <div class="guide-name">pnpm</div>
        <div class="guide-desc">{{ t("runtimeGuide.pnpmDesc") }}</div>
        <button class="btn small primary" @click="emit('open-url', PNPM_URL)">
          {{ t("runtimeGuide.openPnpm") }}
        </button>
      </div>

      <div class="modal-desc guide-note">{{ t("runtimeGuide.note") }}</div>

      <div class="modal-actions">
        <button class="btn" @click="emit('close')">{{ t("runtimeGuide.close") }}</button>
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
  width: 480px; max-width: 92vw; max-height: 86vh; overflow: auto;
  background: var(--panel-bg); border-radius: 10px; padding: 18px 20px;
  box-shadow: 0 16px 48px var(--overlay);
}
.modal h3 { margin: 0 0 10px; font-size: 15px; }
.modal-desc { font-size: 13px; color: var(--text-code); margin-bottom: 14px; line-height: 1.5; }
.guide-block {
  padding: 10px 12px; margin-bottom: 10px; border-radius: 8px;
  background: var(--surface-1); border: 1px solid var(--panel-border);
}
.guide-name { font-weight: 600; font-size: 13px; margin-bottom: 4px; }
.guide-desc { font-size: 12px; color: var(--text-3); line-height: 1.5; margin-bottom: 8px; }
.guide-note { margin-top: 12px; color: var(--text-muted); font-size: 12px; }
.modal-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
</style>
