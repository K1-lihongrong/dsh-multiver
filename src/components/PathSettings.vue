<script setup>
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "../composables/useToast.js";
import { useSharedHomeWarn } from "../composables/useSharedHomeWarn.js";
import { themeMode, setTheme } from "../composables/useTheme.js";
import { langMode, setLang, t } from "../composables/useI18n.js";

// 路径设置 + 维护面板。
// state 由父组件传入（用于显示各目录路径与维护状态）；动作自包含（自己 invoke + emit refresh）。
const props = defineProps({
  state: { type: Object, default: null },
  defaultVersion: { type: String, default: null },
});
const emit = defineEmits(["refresh", "open-dir", "clear-default"]);

const { notify } = useToast();
const { warnSharedHomeEnabled, setWarnSharedHome } = useSharedHomeWarn();

const rootInput = ref("");
const maintBusy = ref("");
const showAllDirs = ref(false); // 「展开全部」：显示 logs/webview/trash/assets

// state 变化时同步根目录输入框（初始 + 应用后刷新都会走到）
watch(
  () => props.state && props.state.root_dir,
  (v) => { if (v != null) rootInput.value = v; },
  { immediate: true }
);

async function applyRoot() {
  try {
    notify(await invoke("set_root", { root: rootInput.value.trim() || null }));
    emit("refresh");
  } catch (e) { notify("" + e); }
}

async function resetRoot() {
  rootInput.value = "";
  await applyRoot();
}

/// 切换「复用官方配置目录（~/.dsh）」
async function toggleOfficialHome(on) {
  if (on) {
    // 防呆：首次开启时明确告知"不删数据、可恢复"
    const path = (props.state && props.state.official_home_dir) || "~/.dsh";
    const msg = t("paths.officialHomeConfirm", { path });
    if (!window.confirm(msg)) return;
    try {
      notify(await invoke("set_use_official_home", { enabled: true }));
      // 目录不存在时不静默：dsh 会自动创建，这里明确提示路径
      emit("refresh");
    } catch (e) {
      notify("" + e);
    }
  } else {
    try {
      notify(await invoke("set_use_official_home", { enabled: false }));
      emit("refresh");
    } catch (e) {
      notify("" + e);
    }
  }
}

function openDir(which) {
  emit("open-dir", which);
}

/// 维护：手动触发
async function doMaintenance(kind) {
  maintBusy.value = kind;
  try {
    notify(await invoke("run_maintenance", { kind }));
    emit("refresh");
  } catch (e) {
    notify("" + e);
  } finally {
    maintBusy.value = "";
  }
}

/// 维护：切换自动维护开关
async function toggleAutoMaint(on) {
  try {
    await invoke("set_auto_maintenance", { enabled: on });
    emit("refresh");
  } catch (e) {
    notify("" + e);
  }
}

/// unix 秒 -> 可读时间
function fmtTs(sec) {
  if (!sec) return t("paths.never");
  const d = new Date(sec * 1000);
  const p = (n) => String(n).padStart(2, "0");
  return d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()) + " " +
    p(d.getHours()) + ":" + p(d.getMinutes());
}
</script>

<template>
  <section class="panel">
    <div class="panel-head">
      <h2>{{ t("paths.title") }}</h2>
      <button class="btn small" @click="$emit('clear-default')" v-if="defaultVersion">{{ t("paths.clearDefault") }}</button>
    </div>

    <div class="field">
      <label>{{ t("paths.rootDir") }}</label>
      <div class="field-row">
        <input v-model="rootInput" :placeholder="t('paths.rootPlaceholder')" />
        <button class="btn primary" @click="applyRoot">{{ t("paths.apply") }}</button>
        <button class="btn" @click="resetRoot">{{ t("paths.reset") }}</button>
      </div>
    </div>

    <label class="checkbox-row">
      <input
        type="checkbox"
        :checked="warnSharedHomeEnabled"
        @change="setWarnSharedHome($event.target.checked)"
      />
      <span>{{ t("paths.warnSharedHome") }}</span>
    </label>

    <label class="checkbox-row">
      <input
        type="checkbox"
        :checked="state && state.use_official_dsh_home"
        @change="toggleOfficialHome($event.target.checked)"
        :disabled="!state"
      />
      <span>{{ t("paths.useOfficialHome") }}</span>
    </label>
    <div class="maint-rules" style="margin-top: -8px; margin-bottom: 14px;" v-if="state && state.use_official_dsh_home">
      <div>{{ t("paths.officialHomePath") }}：{{ state.official_home_dir || "~/.dsh" }}</div>
    </div>

    <div class="paths" v-if="state">
      <div class="path-item" @click="openDir('versions')">
        <span class="path-name">versions</span>
        <span class="path-desc">{{ t("paths.versionsDesc") }}</span>
        <code>{{ state.versions_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('home')">
        <span class="path-name">home</span>
        <span class="path-desc">{{ t("paths.homeDesc") }}</span>
        <code>{{ state.home_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('store')">
        <span class="path-name">store</span>
        <span class="path-desc">{{ t("paths.storeDesc") }}</span>
        <code>{{ state.store_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('cache')">
        <span class="path-name">cache</span>
        <span class="path-desc">{{ t("paths.cacheDesc") }}</span>
        <code>{{ state.cache_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('state')">
        <span class="path-name">state</span>
        <span class="path-desc">{{ t("paths.stateDesc") }}</span>
        <code>{{ state.state_dir }}</code>
      </div>

      <!-- 展开后：额外目录（logs/webview/trash/assets） -->
      <template v-if="showAllDirs && state.extra_dirs">
        <div class="path-item" v-for="d in state.extra_dirs" :key="d.name" @click="openDir(d.name)">
          <span class="path-name">{{ d.name }}</span>
          <span class="path-desc">{{ d.desc }}</span>
          <code>{{ d.path }}</code>
        </div>
      </template>
    </div>
    <div class="paths-toggle">
      <button class="btn small" @click="showAllDirs = !showAllDirs">
        {{ showAllDirs ? t("paths.collapse") : t("paths.expandAll") }}
      </button>
      <span class="hint">{{ t("paths.clickToOpen") }}</span>
    </div>

    <!-- 外观与语言 -->
    <div class="maint">
      <label class="checkbox-row" style="margin-bottom: 8px;">
        <span style="font-weight: 600;">{{ t("paths.appearance") }}</span>
      </label>
      <div class="field-row" style="margin-bottom: 8px;">
        <label style="font-size: 12px; color: var(--text-3); min-width: 52px;">{{ t("paths.theme") }}</label>
        <select class="input" style="flex: 1;" :value="themeMode" @change="setTheme($event.target.value)">
          <option value="system">{{ t("paths.themeSystem") }}</option>
          <option value="light">{{ t("paths.themeLight") }}</option>
          <option value="dark">{{ t("paths.themeDark") }}</option>
        </select>
      </div>
      <div class="field-row">
        <label style="font-size: 12px; color: var(--text-3); min-width: 52px;">{{ t("paths.language") }}</label>
        <select class="input" style="flex: 1;" :value="langMode" @change="setLang($event.target.value)">
          <option value="system">{{ t("paths.langSystem") }}</option>
          <option value="zh-CN">简体中文</option>
          <option value="zh-TW">繁體中文</option>
          <option value="en-US">English</option>
          <option value="ja-JP">日本語</option>
        </select>
      </div>
    </div>

    <!-- 维护 -->
    <div class="maint" v-if="state">
      <label class="checkbox-row">
        <input type="checkbox" :checked="state.maintenance.auto_enabled" @change="toggleAutoMaint($event.target.checked)" />
        <span>{{ t("paths.autoMaintenance") }}</span>
      </label>
      <div class="maint-rules">
        <div>{{ t("paths.rule1") }}</div>
        <div>{{ t("paths.rule2") }}</div>
      </div>
      <div class="maint-status">
        <span>{{ t("paths.lastCleanup") }}：{{ fmtTs(state.maintenance.last_cleanup_at) }}（{{ state.maintenance.last_cleanup_count }}）</span>
        <span>{{ t("paths.lastPrune") }}：{{ fmtTs(state.maintenance.last_prune_at) }}</span>
      </div>
      <div class="maint-actions">
        <button class="btn small" @click="doMaintenance('cleanup')" :disabled="!!maintBusy">{{ maintBusy === 'cleanup' ? t("paths.cleaning") : t("paths.cleanup") }}</button>
        <button class="btn small" @click="doMaintenance('prune')" :disabled="!!maintBusy">{{ maintBusy === 'prune' ? t("paths.pruning") : t("paths.prune") }}</button>
        <button class="btn small" @click="openDir('logs')">{{ t("paths.openLogs") }}</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.field { margin-bottom: 14px; }
.field label { display: block; font-size: 12px; color: var(--text-3); margin-bottom: 6px; }
.field-row { display: flex; gap: 8px; }
.field-row input {
  flex: 1; padding: 8px 12px; border: 1px solid var(--border);
  border-radius: 7px; font-size: 13px; font-family: inherit; outline: none;
}
.field-row input:focus { border-color: var(--accent); }
.field-row select {
  flex: 1; padding: 8px 12px; border: 1px solid var(--border);
  border-radius: 7px; font-size: 13px; font-family: inherit; outline: none;
  background: var(--panel-bg); color: var(--text); cursor: pointer;
}
.field-row select:focus { border-color: var(--accent); }
.checkbox-row {
  display: flex; align-items: center; gap: 8px;
  font-size: 12px; color: var(--text-3); margin: 4px 0 14px; cursor: pointer;
}
.checkbox-row input { cursor: pointer; }
.paths { display: flex; flex-direction: column; gap: 6px; }
.path-item {
  display: flex; align-items: center; gap: 10px;
  padding: 8px 12px; border-radius: 8px; cursor: pointer;
  transition: background .15s;
}
.path-item:hover { background: var(--accent-weak); }
.path-name {
  font-size: 12px; font-weight: 600; color: var(--accent);
  min-width: 64px; font-family: ui-monospace, Consolas, monospace;
}
.path-desc { font-size: 11px; color: var(--text-muted); min-width: 180px; }
.path-item code {
  font-size: 12px; color: var(--text-3); overflow: hidden;
  text-overflow: ellipsis; white-space: nowrap;
}
.hint { color: var(--text-muted); font-size: 13px; padding: 8px 0; }
.paths-toggle { display: flex; align-items: center; gap: 10px; padding: 6px 0 2px; }
.paths-toggle .hint { padding: 0; font-size: 12px; }
.maint { margin-top: 16px; padding-top: 14px; border-top: 1px solid var(--gray-chip); }
.maint-rules { font-size: 12px; color: var(--text-3); line-height: 1.8; margin: 4px 0 10px; }
.maint-status { font-size: 12px; color: var(--text-muted); line-height: 1.8; margin-bottom: 10px; }
.maint-status span { display: block; }
.maint-actions { display: flex; gap: 8px; flex-wrap: wrap; }
.maint-actions .btn { color: var(--accent); border-color: var(--accent-border); }
.maint-actions .btn:hover:not(:disabled) { background: var(--accent-weak); border-color: var(--accent); }
</style>
