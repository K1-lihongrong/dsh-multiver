<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../composables/useI18n.js";
import { logFrontend } from "../api/app.js";

const props = defineProps({
  state: { type: Object, default: null },
});

// ── 无边框窗口控制 ─────────────────────────────────────────────
// decorations:false 后系统标题栏消失，需自绘最小化 / 最大化 / 关闭，
// 并自行处理拖动（带 4px 阈值，让双击仍然可用）。
const isMaximized = ref(false);
let appWin = null;
let appWinBroken = false;
let unlistenWindowResize = null;

function getAppWin() {
  if (appWinBroken) return null;
  if (appWin) return appWin;
  try { appWin = getCurrentWindow(); } catch (_) { appWinBroken = true; return null; }
  return appWin;
}

function reportWindowError(action, e) {
  const msg = "窗口操作失败：" + action + "：" + (e?.message || e);
  try { logFrontend("[window] " + msg).catch(() => {}); } catch (_) {}
  console.error(msg);
}

async function minimizeWindow() {
  try { await getAppWin()?.minimize(); } catch (e) { reportWindowError("minimize", e); }
}
async function toggleMaxWindow() {
  try { await getAppWin()?.toggleMaximize(); } catch (e) { reportWindowError("toggleMaximize", e); }
}
async function closeWindow() {
  try { await getAppWin()?.close(); } catch (e) { reportWindowError("close", e); }
}

function onTopbarMousedown(e) {
  if (e.button !== 0) return;
  if (e.target.closest(".window-controls, button, a, input, textarea, select")) return;
  const win = getAppWin();
  if (!win) return;
  const sx = e.screenX, sy = e.screenY;
  let dragging = false;
  const onMove = (ev) => {
    if (dragging) return;
    if (Math.abs(ev.screenX - sx) > 4 || Math.abs(ev.screenY - sy) > 4) {
      dragging = true;
      cleanup();
      win.startDragging().catch((e) => reportWindowError("startDragging", e));
    }
  };
  const onUp = () => cleanup();
  const cleanup = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

onMounted(() => {
  const win = getAppWin();
  if (!win) return;
  win.isMaximized().then((v) => { isMaximized.value = !!v; }).catch(() => {});
  win.onResized(() => {
    win.isMaximized().then((v) => { isMaximized.value = !!v; }).catch(() => {});
  }).then((u) => { unlistenWindowResize = u; }).catch(() => {});
});

onUnmounted(() => {
  if (unlistenWindowResize) unlistenWindowResize();
});
</script>

<template>
  <div class="app">
    <slot name="launch" />

    <header class="topbar" @mousedown="onTopbarMousedown" @dblclick="toggleMaxWindow">
      <div class="brand">
        <span class="dot"></span>
        <h1>{{ t("app.title") }}</h1>
        <span class="app-version" v-if="state && state.version">v{{ state.version }}</span>
      </div>
      <div class="topbar-right">
        <div class="root-line" v-if="state">
          <span class="label">{{ t("app.rootDir") }}</span>
          <code>{{ state.root_dir }}</code>
        </div>
        <div class="window-controls">
          <button class="wc-btn" @click="minimizeWindow" :title="t('window.minimize')" :aria-label="t('window.minimize')">
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x="1" y="4.5" width="8" height="1" fill="currentColor"/></svg>
          </button>
          <button class="wc-btn" @click="toggleMaxWindow" :title="isMaximized ? t('window.restore') : t('window.maximize')" :aria-label="isMaximized ? t('window.restore') : t('window.maximize')">
            <svg v-if="!isMaximized" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1"/></svg>
            <svg v-else width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <rect x="1" y="3" width="6" height="6" fill="none" stroke="currentColor" stroke-width="1"/>
              <rect x="3" y="1" width="6" height="6" fill="none" stroke="currentColor" stroke-width="1"/>
            </svg>
          </button>
          <button class="wc-btn wc-close" @click="closeWindow" :title="t('window.close')" :aria-label="t('window.close')">
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" stroke-width="1.2"/>
              <line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" stroke-width="1.2"/>
            </svg>
          </button>
        </div>
      </div>
    </header>

    <main class="content">
      <slot />
    </main>

    <slot name="floating" />
  </div>
</template>

<style>
.app { height: 100%; display: flex; flex-direction: column; position: relative; }

.topbar {
  padding: 12px 22px;
  background: var(--panel-bg);
  border-bottom: 1px solid var(--panel-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
  -webkit-user-select: none;
  user-select: none;
  cursor: default;
}
.topbar-right {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-wrap: wrap;
}
.window-controls {
  display: flex;
  gap: 2px;
  align-items: center;
}
.wc-btn {
  width: 32px;
  height: 26px;
  border: none;
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
  border-radius: 4px;
  font-family: inherit;
  padding: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background-color var(--motion-fast) var(--ease-standard),
              color var(--motion-fast) var(--ease-standard),
              transform var(--motion-fast) var(--ease-standard);
}
.wc-btn:hover { background: var(--surface-3); }
.wc-btn:active { background: var(--surface-4); transform: scale(0.96); }
.wc-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.wc-close:hover { background: var(--danger, #e5484d); color: #fff; }
.wc-close:active { background: #b83b3f; color: #fff; }
.brand { display: flex; align-items: center; gap: 10px; }
.dot {
  width: 10px; height: 10px; border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 0 4px var(--accent-glow);
}
.brand h1 { font-size: 16px; margin: 0; font-weight: 600; letter-spacing: 0.2px; }
.app-version { font-size: 11px; color: var(--text-muted); background: var(--surface-3); padding: 1px 6px; border-radius: 8px; font-weight: 500; }
.root-line { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-3); }
.root-line .label { color: var(--text-muted); }
.root-line code {
  background: var(--surface-2); padding: 3px 8px; border-radius: 5px;
  font-size: 12px; color: var(--text-code); max-width: 420px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  font-family: ui-monospace, Consolas, "Courier New", monospace;
}

.content {
  flex: 1; overflow-y: auto; padding: 18px 22px 28px;
  display: flex; flex-direction: column; gap: 16px;
}
</style>
