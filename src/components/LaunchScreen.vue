<script setup>
import { computed, ref, watch, onBeforeUnmount } from "vue";
import { t } from "../composables/useI18n.js";
import { isDark } from "../composables/useTheme.js";
import bgLight from "../assets/loading-bg-light-1280.webp";
import bgDark from "../assets/loading-bg-dark-1280.webp";
import bgError from "../assets/loading-bg-error-1280.webp";
import dshLogo from "../assets/dsh-logo.png";

// v0.2.14：启动加载页（全屏背景 + 底部玻璃面板）。
// 视觉参考设计稿；logo/版本号布局修正；加载动画改为三段式流光。
const props = defineProps({
  version: { type: String, default: "" },
  stage: { type: String, default: "preparing" }, // preparing | waiting | error
  errorText: { type: String, default: "" },
});
const emit = defineEmits(["back", "view-logs"]);

// R-13：错误态背景（设计稿「故障页」，浅色/深色主题共用同一张）。
// 失败切换时先播放约 800ms 的「信号崩坏」干扰（抖动 + 水平条纹），再切故障页。
const glitching = ref(false);
let glitchTimer = 0;
let wasError = false;

watch(
  () => props.stage,
  (s) => {
    if (s === "error" && !wasError) {
      wasError = true;
      glitching.value = true;
      glitchTimer = window.setTimeout(() => { glitching.value = false; }, 800);
    } else if (s !== "error") {
      wasError = false;
      glitching.value = false;
      if (glitchTimer) { window.clearTimeout(glitchTimer); glitchTimer = 0; }
    }
  },
  { immediate: true }
);
onBeforeUnmount(() => { if (glitchTimer) window.clearTimeout(glitchTimer); });

// 干扰期间仍显示正常加载页背景，结束后才换故障页。
const bgSrc = computed(() => {
  if (props.stage === "error" && !glitching.value) return bgError;
  return isDark() ? bgDark : bgLight;
});

const statusText = computed(() => {
  if (props.stage === "error") return t("launch.failed");
  return props.stage === "waiting" ? t("launch.waiting") : t("launch.starting");
});
</script>

<template>
  <div class="splash" :class="{ dark: isDark(), glitching }">
    <div class="splash-bg" aria-hidden="true">
      <img class="splash-bg-img" :src="bgSrc" :class="{ 'bg-enter': stage === 'error' && !glitching }" alt="" />
      <div class="splash-scrim"></div>
      <div v-if="glitching" class="splash-glitch" aria-hidden="true">
        <span class="glitch-bar" v-for="i in 6" :key="i" :style="{ '--i': i }"></span>
      </div>
    </div>

    <section class="splash-stage">
      <div class="splash-panel">
        <!-- 顶部：logo + DSH + 版本号（左），终端动画（右） -->
        <div class="splash-brand">
          <img class="splash-mark" :src="dshLogo" alt="DeepSeek Harness" />
          <span class="splash-wordmark">DSH</span>
          <span class="splash-version" v-if="version">{{ version }}</span>

        </div>

        <!-- 加载动画：光点脉冲 + 流光条 -->
        <div class="splash-loader" v-if="stage !== 'error'" role="progressbar" aria-label="启动加载">
          <div class="splash-dots" aria-hidden="true">
            <span class="dot"></span><span class="dot"></span><span class="dot"></span><span class="dot"></span><span class="dot"></span>
          </div>
          <div class="splash-status">{{ statusText }}</div>
          <div class="splash-track" aria-hidden="true">
            <div class="splash-stream"></div>
          </div>
        </div>

        <!-- 失败：错误 + 操作 -->
        <div class="splash-error" v-else>
          <div class="splash-status error">{{ t("launch.failed") }}</div>
          <div class="splash-error-text">{{ errorText }}</div>
          <div class="splash-actions">
            <button class="splash-btn" @click="emit('back')">{{ t("launch.back") }}</button>
            <button class="splash-btn ghost" @click="emit('view-logs')">{{ t("launch.viewLogs") }}</button>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.splash {
  position: absolute; inset: 0; z-index: 200;
  display: flex; align-items: flex-end; justify-content: center;
  overflow: hidden;
  font-family: Inter, "Noto Sans SC", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif;
}
.splash-bg, .splash-bg-img, .splash-scrim { position: absolute; inset: 0; }
.splash-bg-img { width: 100%; height: 100%; object-fit: cover; object-position: center; }
.splash-scrim {
  background: linear-gradient(180deg, rgba(9,20,40,0) 0%, rgba(9,20,40,0) 38%, rgba(9,20,40,.46) 100%);
}
.splash.dark .splash-scrim {
  background: linear-gradient(180deg, rgba(4,8,18,0) 0%, rgba(4,8,18,0) 36%, rgba(4,8,18,.66) 100%);
}

.splash-stage {
  position: relative; z-index: 1;
  width: min(560px, calc(100% - 48px));
  margin-bottom: clamp(32px, 9vh, 96px);
}
.splash-panel {
  background: rgba(255, 255, 255, 0.66);
  border: 1px solid rgba(255, 255, 255, 0.6);
  border-radius: 16px;
  backdrop-filter: blur(10px) saturate(1.1);
  -webkit-backdrop-filter: blur(10px) saturate(1.1);
  padding: 20px 22px 18px;
  box-shadow: 0 12px 40px rgba(14, 26, 46, 0.12);
}
.splash.dark .splash-panel {
  background: rgba(18, 26, 46, 0.64);
  border-color: rgba(122, 160, 220, 0.22);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.4);
}

/* 顶部：logo + 字标（左），版本号（右） */
.splash-brand { display: flex; align-items: center; gap: 10px; }
.splash-mark { width: 26px; height: 26px; flex: none; border-radius: 7px; }
.splash-wordmark {
  font-weight: 600; font-size: 15px; letter-spacing: .16em;
  color: #0e1a2e;
}
.splash.dark .splash-wordmark { color: #e9f0ff; }
.splash-version {
  margin-left: auto;
  font-size: 12px; color: #56688a;
  border: 1px solid #c6d5ec; border-radius: 4px;
  padding: 2px 8px; font-variant-numeric: tabular-nums;
}
.splash.dark .splash-version { color: #93a6c4; border-color: #24314e; }

/* 加载动画：5 光点脉冲 + 流光条 */
.splash-loader { margin-top: 16px; }
.splash-dots { display: flex; gap: 7px; align-items: center; height: 10px; }
.splash-dots .dot {
  width: 8px; height: 8px; border-radius: 50%;
  background: #2f6bdd;
  opacity: .28;
  animation: dotPulse 1.5s ease-in-out infinite;
}
.splash.dark .splash-dots .dot { background: #5a9bff; }
.splash-dots .dot:nth-child(1) { animation-delay: 0s; }
.splash-dots .dot:nth-child(2) { animation-delay: .15s; }
.splash-dots .dot:nth-child(3) { animation-delay: .3s; }
.splash-dots .dot:nth-child(4) { animation-delay: .45s; }
.splash-dots .dot:nth-child(5) { animation-delay: .6s; }
@keyframes dotPulse {
  0%, 100% { opacity: .22; transform: scale(.85); }
  50% { opacity: 1; transform: scale(1.15); }
}
.splash-track {
  position: relative; margin-top: 14px; height: 3px; border-radius: 2px;
  background: rgba(14, 26, 46, 0.12); overflow: hidden;
}
.splash.dark .splash-track { background: rgba(233, 240, 255, 0.14); }
.splash-stream {
  position: absolute; top: 0; bottom: 0; width: 40%;
  border-radius: 2px;
  background: linear-gradient(90deg, transparent, #2f6bdd 40%, #5a9bff 60%, transparent);
  animation: streamMove 1.6s ease-in-out infinite;
}
.splash.dark .splash-stream {
  background: linear-gradient(90deg, transparent, #5a9bff 40%, #8fbaff 60%, transparent);
}
@keyframes streamMove {
  0% { left: -40%; }
  100% { left: 100%; }
}
/* R-13：失败切换的「信号崩坏」干扰（约 800ms），随后故障页淡入 */
.splash.glitching .splash-stage { animation: glitchShake .8s steps(2, end) both; }
.splash-glitch {
  position: absolute; inset: 0; z-index: 2; pointer-events: none;
  mix-blend-mode: screen;
}
.splash-glitch .glitch-bar {
  position: absolute; left: 0; right: 0; height: 6%;
  top: calc((var(--i) - 1) * 16%);
  background: linear-gradient(90deg, transparent, rgba(120,200,255,.55), rgba(255,255,255,.75), rgba(120,200,255,.55), transparent);
  opacity: 0;
  animation: glitchBar .8s steps(3, end) both;
  animation-delay: calc(var(--i) * 45ms);
}
@keyframes glitchShake {
  0%, 100% { transform: translate3d(0, 0, 0); filter: none; }
  12% { transform: translate3d(-3px, 1px, 0); filter: contrast(1.15) saturate(1.2); }
  28% { transform: translate3d(4px, -2px, 0); filter: contrast(1.25) brightness(1.08); }
  46% { transform: translate3d(-5px, 2px, 0); filter: contrast(1.1); }
  64% { transform: translate3d(3px, -1px, 0); filter: contrast(1.3) brightness(1.12); }
  82% { transform: translate3d(-2px, 1px, 0); filter: contrast(1.1); }
}
@keyframes glitchBar {
  0% { opacity: 0; transform: translateX(-12%); }
  20% { opacity: .9; transform: translateX(6%); }
  45% { opacity: .35; transform: translateX(-4%); }
  70% { opacity: .85; transform: translateX(3%); }
  100% { opacity: 0; transform: translateX(0); }
}
/* 故障页淡入 + 轻微回落 */
.splash-bg-img.bg-enter { animation: bgEnter .26s ease-out both; }
@keyframes bgEnter {
  from { opacity: .2; transform: scale(1.02); }
  to { opacity: 1; transform: scale(1); }
}

@media (prefers-reduced-motion: reduce) {
  .splash-dots .dot { animation: none; opacity: .6; }
  .splash-stream { animation: none; left: 30%; opacity: .5; }
  .splash.glitching .splash-stage { animation: none; }
  .splash-glitch { display: none; }
  .splash-bg-img.bg-enter { animation: none; }
}

.splash-status {
  margin-top: 10px; font-size: 15px; color: #56688a; letter-spacing: .01em;
}
.splash.dark .splash-status { color: #93a6c4; }
.splash-status.error { color: #e5484d; font-weight: 600; }

.splash-error { margin-top: 16px; }
.splash-error-text {
  margin-top: 8px; max-height: 26vh; overflow-y: auto;
  font-size: 12px; color: #56688a; white-space: pre-wrap; word-break: break-all;
  background: rgba(14, 26, 46, 0.05); padding: 10px 12px; border-radius: 8px;
}
.splash.dark .splash-error-text { color: #93a6c4; background: rgba(255,255,255,0.06); }
.splash-actions { display: flex; gap: 10px; margin-top: 12px; }
.splash-btn {
  padding: 7px 16px; border-radius: 6px; font-size: 13px; cursor: pointer;
  border: 1px solid #c6d5ec; background: #2f6bdd; color: #fff; border-color: #2f6bdd;
}
.splash-btn.ghost { background: transparent; color: #0e1a2e; border-color: #c6d5ec; }
.splash.dark .splash-btn { background: #5a9bff; color: #06101f; border-color: #5a9bff; }
.splash.dark .splash-btn.ghost { background: transparent; color: #e9f0ff; border-color: #24314e; }

@media (max-width: 640px) {
  .splash-stage { width: calc(100% - 32px); margin-bottom: 24px; }
}
</style>
