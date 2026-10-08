<script setup>
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "../composables/useToast.js";
import { useSharedHomeWarn } from "../composables/useSharedHomeWarn.js";
import { themeMode, setTheme } from "../composables/useTheme.js";

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
  if (!sec) return "从未";
  const d = new Date(sec * 1000);
  const p = (n) => String(n).padStart(2, "0");
  return d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()) + " " +
    p(d.getHours()) + ":" + p(d.getMinutes());
}
</script>

<template>
  <section class="panel">
    <div class="panel-head">
      <h2>路径设置</h2>
      <button class="btn small" @click="$emit('clear-default')" v-if="defaultVersion">清除默认</button>
    </div>

    <div class="field">
      <label>数据根目录</label>
      <div class="field-row">
        <input v-model="rootInput" placeholder="留空则使用软件所在目录" />
        <button class="btn primary" @click="applyRoot">应用</button>
        <button class="btn" @click="resetRoot">默认</button>
      </div>
    </div>

    <label class="checkbox-row">
      <input
        type="checkbox"
        :checked="warnSharedHomeEnabled"
        @change="setWarnSharedHome($event.target.checked)"
      />
      <span>运行「共享 home」版本时提示（多版本混用可能错配）</span>
    </label>

    <div class="paths" v-if="state">
      <div class="path-item" @click="openDir('versions')">
        <span class="path-name">versions</span>
        <span class="path-desc">已安装的各版本 dsh</span>
        <code>{{ state.versions_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('home')">
        <span class="path-name">home</span>
        <span class="path-desc">dsh 数据目录（相当于 ~/.dsh）</span>
        <code>{{ state.home_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('store')">
        <span class="path-name">store</span>
        <span class="path-desc">pnpm 依赖仓库（硬链接省磁盘）</span>
        <code>{{ state.store_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('cache')">
        <span class="path-name">cache</span>
        <span class="path-desc">pnpm 下载/元数据缓存（可安全删除）</span>
        <code>{{ state.cache_dir }}</code>
      </div>
      <div class="path-item" @click="openDir('state')">
        <span class="path-name">state</span>
        <span class="path-desc">pnpm 运行状态（可安全删除）</span>
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
        {{ showAllDirs ? "收起" : "展开全部目录" }}
      </button>
      <span class="hint">点击任意路径可在资源管理器中打开</span>
    </div>

    <!-- 外观 -->
    <div class="maint">
      <label class="checkbox-row" style="margin-bottom: 8px;">
        <span style="font-weight: 600;">外观</span>
      </label>
      <div class="field-row">
        <label style="font-size: 12px; color: var(--text-3);">主题</label>
        <select class="input" style="flex: 1;" :value="themeMode" @change="setTheme($event.target.value)">
          <option value="system">跟随系统</option>
          <option value="light">亮色</option>
          <option value="dark">暗色</option>
        </select>
      </div>
    </div>

    <!-- 维护 -->
    <div class="maint" v-if="state">
      <label class="checkbox-row">
        <input type="checkbox" :checked="state.maintenance.auto_enabled" @change="toggleAutoMaint($event.target.checked)" />
        <span>启动时自动维护</span>
      </label>
      <div class="maint-rules">
        <div>· 每次启动清理<strong>孤立缓存</strong>（版本已卸载的 WebView 残留）</div>
        <div>· 距上次 ≥ 7 天时，自动<strong>回收依赖仓库</strong>（pnpm store prune）</div>
      </div>
      <div class="maint-status">
        <span>上次清理孤立缓存：{{ fmtTs(state.maintenance.last_cleanup_at) }}（{{ state.maintenance.last_cleanup_count }} 项）</span>
        <span>上次回收依赖仓库：{{ fmtTs(state.maintenance.last_prune_at) }}</span>
      </div>
      <div class="maint-actions">
        <button class="btn small" @click="doMaintenance('cleanup')" :disabled="!!maintBusy">{{ maintBusy === 'cleanup' ? '清理中...' : '清理孤立缓存' }}</button>
        <button class="btn small" @click="doMaintenance('prune')" :disabled="!!maintBusy">{{ maintBusy === 'prune' ? '回收中...' : '回收依赖仓库' }}</button>
        <button class="btn small" @click="openDir('logs')">打开日志目录</button>
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
