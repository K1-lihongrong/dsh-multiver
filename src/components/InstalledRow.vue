<script setup>
import { computed } from "vue";
import { t } from "../composables/useI18n.js";
import InstanceMenu from "./InstanceMenu.vue";
import NotesPanel from "./NotesPanel.vue";
import DsPluginPanel from "./DsPluginPanel.vue";

// A2-9: 单个已安装版本行。父组件保留选择/扫描/卸载等编排状态，
// 行组件只负责该行的展示与动作触发，避免 InstalledList 继续膨胀。
const props = defineProps({
  item: { type: Object, required: true },
  selected: { type: Boolean, default: false },
  open: { type: Boolean, default: false },
  isScanning: { type: Boolean, default: false },
  queued: { type: Boolean, default: false },
  flash: { type: Boolean, default: false },
  progress: { type: Object, default: null },
  scanPending: { type: Boolean, default: false },
  sizeInfo: { type: Object, default: null },
  cached: { type: Boolean, default: false },
  fadingOut: { type: Boolean, default: false },
  batchBusy: { type: Boolean, default: false },
  uninstallingVersion: { type: String, default: "" },
  runningVersion: { type: String, default: "" },
  pluginPanelOpen: { type: Boolean, default: false },
  notesOpen: { type: Boolean, default: false },
  menuOpen: { type: Boolean, default: false },
  instances: { type: Array, default: () => [] },
  selectedTarget: { type: String, default: null },
  runningTargets: { type: Array, default: () => [] },
  useOfficialHome: { type: Boolean, default: false },
});
const emit = defineEmits([
  "select",
  "toggle-notes",
  "run",
  "open-browser",
  "set-default",
  "shortcut",
  "toggle-plugin",
  "scan-size",
  "toggle-isolated",
  "select-target",
  "create-instance",
  "rename-instance",
  "remove-instance",
  "open-instance-dir",
  "uninstall",
  "close-plugin",
  "close-notes",
  "open-url",
  "toggle-menu",
  "close-menu",
]);

const isBlocked = computed(
  () => props.uninstallingVersion === props.item.version || (props.batchBusy && props.selected),
);
const isUninstalling = computed(() => props.uninstallingVersion === props.item.version);
const targetInstance = computed(
  () => props.instances.find((i) => i.id === props.selectedTarget) || null
);
const targetRunningOn = computed(() => {
  if (!targetInstance.value) return "";
  const hit = props.runningTargets.find(
    (x) => x.kind === "instance" && x.instance === targetInstance.value.id
  );
  return hit ? hit.version : "";
});
const sharedRunning = computed(() =>
  props.runningTargets.some(
    (x) => x.kind === "shared" && x.version === props.item.version
  )
);
const isRunning = computed(() =>
  targetInstance.value ? !!targetRunningOn.value : sharedRunning.value
);
const runLabel = computed(() => {
  if (targetInstance.value) {
    return targetRunningOn.value
      ? t("instances.focusInstance", { name: targetInstance.value.name })
      : t("instances.runInstance", { name: targetInstance.value.name });
  }
  return sharedRunning.value ? t("instances.focus") : t("installed.run");
});
const runLocked = computed(() => isBlocked.value || !!props.runningVersion);

function runMain() {
  emit("run", {
    version: props.item.version,
    instance: targetInstance.value ? targetInstance.value.id : null,
  });
}
function closeMenu() {
  emit("close-menu");
}
function onMenuRun(payload) {
  closeMenu();
  emit("run", payload);
}
function onSelectTarget(instance) {
  emit("select-target", { version: props.item.version, instance });
}
function onOpenInstanceDir(instance) {
  closeMenu();
  emit("open-instance-dir", { version: props.item.version, instance });
}
function onRenameInstance(inst) {
  closeMenu();
  emit("rename-instance", inst);
}
function onRemoveInstance(inst) {
  closeMenu();
  emit("remove-instance", inst);
}
function onCreateInstance() {
  closeMenu();
  emit("create-instance", props.item.version);
}

function fmtSize(bytes) {
  if (bytes == null) return "";
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  if (bytes < 1024 * 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
  return (bytes / 1024 / 1024 / 1024).toFixed(2) + " GB";
}

function filePercent(p) {
  return p && p.fileTotal ? Math.min(100, Math.round((p.done / p.fileTotal) * 100)) : 0;
}
</script>

<template>
  <li class="ver-item"
    :class="{
      'item-selected': selected,
      'item-scanning': isScanning,
      'item-queued': queued,
      'item-flash': flash,
    }">
    <input type="checkbox" class="sel-box" :checked="selected" :disabled="batchBusy" @change="emit('select')" />
    <div class="ver-main">
      <span class="ver-num ver-num-clickable" :title="t('installed.clickNotes')" @click="emit('toggle-notes')">
        {{ item.version }}<span class="ver-caret">{{ notesOpen ? "▼" : "▶" }}</span>
      </span>
      <span class="badge badge-running" v-if="isRunning" :title="t('installed.openWindowTip')">{{ t("instances.running") }}</span>
      <span class="badge" v-if="item.is_default">{{ t("installed.default") }}</span>
      <span class="badge badge-iso" v-if="selectedTarget">{{ t("installed.isolated") }}</span>
      <span class="badge badge-shared" v-else :title="t('installed.sharedHomeTip')">{{ t("installed.sharedHome") }}</span>
      <span class="ver-meta" v-if="item.installed_at">{{ t("installed.installedAt") }} {{ item.installed_at }}</span>
      <span class="ver-meta" v-if="sizeInfo != null">
        {{ t("installed.usage") }} {{ fmtSize(sizeInfo.total) }}
        <template v-if="sizeInfo.shared_count">
          ({{ t("installed.reused") }} {{ fmtSize(sizeInfo.shared_size) }} / {{ fmtSize(sizeInfo.exclusive_size) }})
        </template>
      </span>
      <span class="badge badge-cached" v-if="cached" :class="{ 'cached-fade': fadingOut }">{{ t("installed.scanCached") }}</span>
      <!-- 单版本扫描：行内文件级进度条 -->
      <span class="ver-inline-progress" v-if="isScanning && progress">
        <span class="ver-inline-bar">
          <span v-if="progress.stage === 'counting'" class="scan-bar-indeterminate"></span>
          <span v-else class="ver-inline-fill" :style="{ width: filePercent(progress) + '%' }"></span>
        </span>
        <span class="ver-inline-pct">{{ progress.stage === 'counting' ? (progress.done ? t("installed.scanCountingN", { n: progress.done }) : t("installed.scanCounting")) : filePercent(progress) + '%' }}</span>
      </span>
    </div>
    <div class="ver-actions">
      <div class="run-wrap">
        <button class="btn primary run-main" @click="runMain" :disabled="runLocked">{{ runLabel }}</button>
        <button class="btn run-caret" @click.stop="emit('toggle-menu')" :disabled="isBlocked" :aria-label="t('instances.panelTitle')" :aria-expanded="menuOpen">▾</button>
        <InstanceMenu
          v-if="menuOpen"
          :version="item.version"
          :instances="instances"
          :selected-target="selectedTarget"
          :running-targets="runningTargets"
          :use-official-home="useOfficialHome"
          @close="emit('close-menu')"
          @select-target="onSelectTarget"
          @run="onMenuRun"
          @open-dir="onOpenInstanceDir"
          @rename="onRenameInstance"
          @remove="onRemoveInstance"
          @create="onCreateInstance"
        />
      </div>
      <button class="btn" @click="emit('open-browser', { version: item.version, instance: targetInstance ? targetInstance.id : null })" :disabled="isBlocked" :title="t('installed.openBrowserTip')">{{ t("installed.openBrowser") }}</button>
      <button class="btn" @click="emit('set-default')" :disabled="item.is_default || isBlocked">{{ t("installed.setDefault") }}</button>
      <button class="btn" @click="emit('shortcut')" :disabled="isBlocked" :title="t('installed.shortcutTip')">{{ t("installed.shortcut") }}</button>
      <button
        class="btn btn-plugin-toggle"
        :class="{ 'is-open': pluginPanelOpen }"
        @click="emit('toggle-plugin')"
        :disabled="isBlocked"
        :aria-expanded="pluginPanelOpen"
      >{{ t("dsplugins.button") }}</button>
      <button class="btn" @click="emit('scan-size')" :disabled="scanPending || isBlocked" :title="t('installed.scanTip')">
        {{ isScanning ? t("installed.scanning") : (sizeInfo != null ? t("installed.rescanSize") : t("installed.scanSize")) }}
      </button>
      <button class="btn danger" @click="emit('uninstall')" :disabled="!!uninstallingVersion || batchBusy || scanPending">{{ isUninstalling ? t("installed.uninstalling") : t("installed.uninstall") }}</button>
    </div>
    <DsPluginPanel
      v-if="pluginPanelOpen"
      :version="item.version"
      @close="emit('close-plugin')"
    />
    <NotesPanel
      v-if="notesOpen"
      :version="item.version"
      inline
      @close="emit('close-notes')"
      @open-url="emit('open-url', $event)"
    />
  </li>
</template>

<style scoped>
.ver-item {
  display: flex; align-items: center; flex-wrap: wrap;
  gap: 6px 12px;
  padding: 10px 14px; border: 1px solid var(--surface-5); border-radius: 9px;
  background: var(--panel-bg);
  transition: border-color var(--motion-base) var(--ease-standard),
              background-color var(--motion-base) var(--ease-standard);
}
.ver-item:hover { border-color: var(--gray-scroll); background: var(--panel-bg); }
.ver-item.item-selected { border-color: var(--accent); background: var(--accent-weak-2); }
.sel-box { flex-shrink: 0; cursor: pointer; }
.ver-main { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; flex: 1 1 auto; }
.ver-num { font-weight: 600; font-size: 14px; font-variant-numeric: tabular-nums; }
.ver-num-clickable { cursor: pointer; user-select: none; }
.ver-num-clickable:hover { color: var(--accent); }
.ver-caret { font-size: 10px; margin-left: 3px; color: var(--text-muted); }
.ver-meta { color: var(--text-muted); font-size: 11px; }
.ver-actions { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; justify-content: flex-start; }
.run-wrap { position: relative; display: inline-flex; }
.run-main { border-top-right-radius: 0; border-bottom-right-radius: 0; }
.run-caret {
  border-top-left-radius: 0; border-bottom-left-radius: 0;
  padding-left: 7px; padding-right: 7px; margin-left: -1px;
}
.run-caret:hover:not(:disabled) { border-color: var(--accent-border); color: var(--accent); }

/* 插件面板展开按钮：展开时显示浅主色底 + 描边，三角箭头转 90°。 */
.btn-plugin-toggle::before {
  content: "";
  display: inline-block;
  width: 0; height: 0;
  margin-right: 5px;
  vertical-align: middle;
  border-left: 4px solid currentColor;
  border-top: 3px solid transparent;
  border-bottom: 3px solid transparent;
  opacity: .55;
  transition: transform var(--motion-fast) var(--ease-standard),
              opacity var(--motion-fast) var(--ease-standard);
}
.btn-plugin-toggle.is-open::before { transform: rotate(90deg); opacity: 1; }
.btn-plugin-toggle.is-open {
  background: var(--accent-weak);
  border-color: var(--accent-border);
  color: var(--accent);
}
.btn-plugin-toggle.is-open:hover:not(:disabled) {
  background: var(--accent-weak-2);
  border-color: var(--accent);
}

@keyframes scan-breathe {
  0%, 100% { border-color: var(--accent); box-shadow: 0 0 0 0 rgba(79, 110, 247, 0); }
  50% { border-color: var(--accent); box-shadow: 0 0 0 3px rgba(79, 110, 247, .20); }
}
.ver-item.item-scanning { animation: scan-breathe 1.4s ease-in-out infinite; }

@keyframes scan-flash {
  0% { background-color: rgba(34, 197, 94, .45); }
  100% { background-color: rgba(34, 197, 94, 0); }
}
.ver-item.item-flash { animation: scan-flash 1.2s ease-out; }
.ver-item.item-queued { background: var(--surface-2); opacity: .6; }

.scan-bar-indeterminate {
  display: block;
  height: 100%; width: 40%;
  background: var(--accent);
  border-radius: 3px;
  animation: scan-indeterminate 1s ease-in-out infinite;
}
@keyframes scan-indeterminate {
  0% { transform: translateX(-120%); }
  100% { transform: translateX(320%); }
}

.badge-cached {
  background: rgba(34, 197, 94, .16); color: #22a55a;
  font-size: 11px; padding: 1px 7px; border-radius: 9px;
  transition: opacity 1.2s ease;
}
.badge-cached.cached-fade { opacity: 0; }

.ver-inline-progress { display: inline-flex; align-items: center; gap: 6px; }
.ver-inline-bar {
  display: inline-block; width: 80px; height: 5px;
  background: var(--surface-2); border-radius: 3px; overflow: hidden;
  position: relative;
}
.ver-inline-fill {
  display: block; height: 100%; background: var(--accent);
  border-radius: 3px; transition: width .2s ease;
}
.ver-inline-pct { font-size: 11px; color: var(--accent); font-variant-numeric: tabular-nums; }
</style>
