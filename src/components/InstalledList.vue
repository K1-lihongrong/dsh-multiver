<script setup>
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useToast } from "../composables/useToast.js";
import { useSharedHomeWarn } from "../composables/useSharedHomeWarn.js";
import { useSizeScan } from "../composables/useSizeScan.js";
import IsolatedMenu from "./IsolatedMenu.vue";
import NotesPanel from "./NotesPanel.vue";
import { t } from "../composables/useI18n.js";

// 已安装版本列表（含批量操作、占用扫描、隔离、更新说明）。
// installed 由父组件传入；动作自包含（自己 invoke），完成后 emit refresh 让父组件重拉列表。
const props = defineProps({
  installed: { type: Array, default: () => [] },
});
const emit = defineEmits(["refresh", "open-url"]);

const { notify } = useToast();
const { warnSharedHomeEnabled } = useSharedHomeWarn();
const { verSizes, scanning, scanningVer, progress, scanAll, scanOne, flashVersions } = useSizeScan();

/// 版本级进度百分比（0-100）
const verPercent = (p) => (p && p.total ? Math.round((p.index / p.total) * 100) : 0);
/// 文件级进度百分比（0-100）
const filePercent = (p) => (p && p.fileTotal ? Math.min(100, Math.round((p.done / p.fileTotal) * 100)) : 0);

const runningVersion = ref(""); // 正在启动的版本号
const uninstalling = ref("");   // 正在卸载的版本号
const installedNoteVer = ref(null); // 当前展开更新说明的版本号

// ---- 批量操作 ----
const selected = ref(new Set());
const batchBusy = ref(false);

function toggleSelect(name) {
  const s = new Set(selected.value);
  if (s.has(name)) s.delete(name); else s.add(name);
  selected.value = s;
}
function isSelected(name) { return selected.value.has(name); }
function isBlocked(name) {
  return uninstalling.value === name || (batchBusy.value && selected.value.has(name));
}
function toggleSelectAll(on) {
  const s = new Set(selected.value);
  for (const it of props.installed) {
    if (on) s.add(it.version); else s.delete(it.version);
  }
  selected.value = s;
}
function clearSelection() { selected.value = new Set(); }

function fmtSize(bytes) {
  if (bytes == null) return "";
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  if (bytes < 1024 * 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
  return (bytes / 1024 / 1024 / 1024).toFixed(2) + " GB";
}

// ---- 单版本操作 ----
async function scanVersionSize(v) {
  try { await scanOne(v); } catch (e) { notify("" + e); }
}

/// 扫描全部版本（两段式：先秒出总量，后台补精确）。
async function scanAllSizes() {
  try { await scanAll(); } catch (e) { notify("" + e); }
}

/// 运行非隔离（共享 home）版本前的提示。返回 false 表示用户取消。
async function confirmSharedHome(v) {
  if (!warnSharedHomeEnabled.value) return true;
  const ver = props.installed.find((x) => x.version === v);
  if (!ver || ver.isolated) return true; // 隔离版本无此风险
  return await ask(
    t("installed.sharedHomeConfirm", { v }),
    { title: t("installed.sharedHomeTitle"), kind: "warning" }
  );
}

async function run(v) {
  if (!(await confirmSharedHome(v))) return;
  if (runningVersion.value) return;
  runningVersion.value = v;
  notify(t("installed.starting", { v }));
  try { notify(await invoke("run_version", { version: v })); }
  catch (e) { notify("" + e); }
  finally { runningVersion.value = ""; }
}

async function openInBrowser(v) {
  if (!(await confirmSharedHome(v))) return;
  try { notify(await invoke("open_in_browser", { version: v })); }
  catch (e) { notify("" + e); }
}

async function setDefault(v) {
  try {
    notify(await invoke("set_default", { version: v }));
    emit("refresh");
  } catch (e) {
    const msg = String(e);
    // GAP-014：目标位置已有"非本管理器"的 dsh 命令 → 确认是否覆盖
    const prefix = "DSH_CONFLICT:";
    if (msg.startsWith(prefix)) {
      const path = msg.slice(prefix.length);
      const ok = await ask(t("installed.dshConflict", { path }), { title: t("installed.dshConflictTitle"), kind: "warning" });
      if (ok) {
        try {
          notify(await invoke("set_default", { version: v, force: true }));
          emit("refresh");
        } catch (e2) { notify("" + e2); }
      }
    } else {
      notify(msg);
    }
  }
}

async function createShortcut(v) {
  try { notify(await invoke("create_shortcut", { version: v })); }
  catch (e) { notify("" + e); }
}

async function toggleIsolated(v, current) {
  try {
    notify(await invoke("set_isolated", { version: v, isolated: !current }));
    emit("refresh");
  } catch (e) { notify("" + e); }
}

async function uninstall(v) {
  if (uninstalling.value) return;
  if (!(await ask(t("installed.uninstallConfirm", { v }), { title: t("installed.uninstallTitle"), kind: "warning" }))) return;
  uninstalling.value = v;
  notify(t("installed.uninstallingMsg", { v }));
  try {
    notify(await invoke("uninstall_version", { version: v }));
    emit("refresh");
  } catch (e) { notify("" + e); }
  finally { uninstalling.value = ""; }
}

/// 批量卸载已选版本
async function batchUninstall() {
  const names = Array.from(selected.value);
  if (!names.length) return notify(t("installed.nothingSelected"));
  if (!(await ask(t("installed.batchConfirm", { n: names.length }), { title: t("installed.batchUninstall"), kind: "warning" }))) return;
  batchBusy.value = true;
  let okCount = 0;
  const failed = [];
  for (let i = 0; i < names.length; i++) {
    const name = names[i];
    notify(t("installed.batchProgress", { i: i + 1, n: names.length, name }));
    uninstalling.value = name;
    try { await invoke("uninstall_version", { version: name }); okCount++; }
    catch (e) { failed.push(name + ": " + e); }
  }
  emit("refresh");
  uninstalling.value = "";
  selected.value = new Set();
  batchBusy.value = false;
  if (failed.length) {
    notify(t("installed.batchDoneFail", { ok: okCount, fail: failed.length }) + "\n" + failed.join("\n"));
  } else {
    notify(t("installed.batchDone", { n: okCount }));
  }
}

function toggleInstalledNote(v) {
  installedNoteVer.value = installedNoteVer.value === v ? null : v;
}
</script>

<template>
  <section class="panel">
    <div class="panel-head">
      <h2>{{ t("installed.title") }}</h2>
      <span class="count" v-if="installed.length">{{ installed.length }}</span>
      <button
        v-if="installed.length"
        class="btn small scan-all-btn"
        @click="scanAllSizes"
        :disabled="scanning || batchBusy"
      >{{ scanning ? t("installed.scanningAll") : t("installed.scanAll") }}</button>
    </div>

    <div class="scan-progress-wrap" v-if="scanning && progress">
      <div class="scan-progress-label">
        {{ t("installed.scanProgress", { i: progress.index, n: progress.total, v: progress.version }) }}
        <span v-if="progress.fileTotal" class="scan-pct">{{ filePercent(progress) }}%</span>
      </div>
      <div class="scan-bar"><div class="scan-bar-fill" :style="{ width: verPercent(progress) + '%' }"></div></div>
      <div class="scan-bar scan-bar-file"><div class="scan-bar-fill file" :style="{ width: filePercent(progress) + '%' }"></div></div>
    </div>

    <div v-if="!installed.length" class="empty">
      {{ t("installed.empty") }}
    </div>

    <ul class="ver-list" v-else>
      <li v-for="v in installed" :key="v.version" class="ver-item"
        :class="{
          'item-selected': isSelected(v.version),
          'item-scanning': (scanning && progress && progress.version === v.version) || scanningVer === v.version,
          'item-flash': flashVersions.has(v.version),
        }">
        <input type="checkbox" class="sel-box" :checked="isSelected(v.version)" :disabled="batchBusy" @change="toggleSelect(v.version)" />
        <div class="ver-main">
          <span class="ver-num ver-num-clickable" :title="t('installed.clickNotes')" @click="toggleInstalledNote(v.version)">
            {{ v.version }}<span class="ver-caret">{{ installedNoteVer === v.version ? "▾" : "▸" }}</span>
          </span>
          <span class="badge" v-if="v.is_default">{{ t("installed.default") }}</span>
          <span class="badge badge-iso" v-if="v.isolated">{{ t("installed.isolated") }}</span>
          <span class="badge badge-shared" v-else :title="t('installed.sharedHomeTip')">{{ t("installed.sharedHome") }}</span>
          <span class="ver-meta" v-if="v.installed_at">{{ t("installed.installedAt") }} {{ v.installed_at }}</span>
          <span class="ver-meta" v-if="verSizes[v.version] != null">
            {{ t("installed.usage") }} {{ fmtSize(verSizes[v.version].total) }}
            <template v-if="verSizes[v.version].shared_count">
              ({{ t("installed.reused") }} {{ fmtSize(verSizes[v.version].shared_size) }} / {{ t("installed.exclusive") }} {{ fmtSize(verSizes[v.version].exclusive_size) }})
            </template>
          </span>
        </div>
        <div class="ver-actions">
          <button class="btn primary" @click="run(v.version)" :disabled="!!runningVersion || isBlocked(v.version)">{{ runningVersion === v.version ? t("installed.running") : t("installed.run") }}</button>
          <button class="btn" @click="openInBrowser(v.version)" :disabled="isBlocked(v.version)" :title="t('installed.openBrowserTip')">{{ t("installed.openBrowser") }}</button>
          <button class="btn" @click="setDefault(v.version)" :disabled="v.is_default || isBlocked(v.version)">{{ t("installed.setDefault") }}</button>
          <button class="btn" @click="createShortcut(v.version)" :disabled="isBlocked(v.version)" :title="t('installed.shortcutTip')">{{ t("installed.shortcut") }}</button>
          <button class="btn" @click="scanVersionSize(v.version)" :disabled="scanningVer === v.version || isBlocked(v.version)" :title="t('installed.scanTip')">
            {{ scanningVer === v.version ? t("installed.scanning") : (verSizes[v.version] != null ? t("installed.rescanSize") : t("installed.scanSize")) }}
          </button>
          <IsolatedMenu
            v-if="v.isolated"
            :version="v.version"
            :disabled="isBlocked(v.version)"
            @toggle-isolated="toggleIsolated(v.version, $event)"
          />
          <button
            v-else
            class="btn"
            @click="toggleIsolated(v.version, false)"
            :disabled="isBlocked(v.version)"
            :title="t('installed.isolateTip')"
          >{{ t("installed.isolate") }}</button>
          <button class="btn danger" @click="uninstall(v.version)" :disabled="!!uninstalling || batchBusy">{{ uninstalling === v.version ? t("installed.uninstalling") : t("installed.uninstall") }}</button>
        </div>
        <NotesPanel
          v-if="installedNoteVer === v.version"
          :version="v.version"
          inline
          @close="installedNoteVer = null"
          @open-url="emit('open-url', $event)"
        />
      </li>
    </ul>

    <!-- 批量操作栏 -->
    <div class="batch-bar" v-if="installed.length">
      <label class="batch-select-all">
        <input type="checkbox"
          :checked="installed.length > 0 && installed.every((x) => isSelected(x.version))"
          :disabled="batchBusy"
          @change="toggleSelectAll($event.target.checked)" />
        {{ t("installed.selectAll") }}
      </label>
      <template v-if="selected.size">
        <span class="batch-count">{{ t("installed.selected") }} {{ selected.size }} {{ t("installed.items") }}</span>
        <button class="btn small" @click="clearSelection" :disabled="batchBusy">{{ t("installed.clearSelection") }}</button>
        <button class="btn danger small" @click="batchUninstall" :disabled="batchBusy">
          {{ batchBusy ? t("installed.batchUninstalling") : t("installed.batchUninstall") }}
        </button>
      </template>
    </div>
  </section>
</template>

<style scoped>
.count {
  background: var(--gray-chip); color: var(--text-3); font-size: 12px; font-weight: 600;
  padding: 1px 8px; border-radius: 10px; margin-left: 8px;
}
.empty { color: var(--text-muted); font-size: 13px; padding: 8px 0; }
.ver-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.ver-item {
  display: flex; align-items: center; flex-wrap: wrap;
  gap: 6px 12px;
  padding: 10px 14px; border: 1px solid var(--surface-5); border-radius: 9px;
  background: var(--panel-bg); transition: border-color .15s, background .15s;
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
.batch-bar {
  display: flex; align-items: center; gap: 12px; flex-wrap: wrap;
  margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--gray-chip);
}
.batch-select-all { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--text-3); cursor: pointer; }
.batch-count { font-size: 13px; color: var(--accent); font-weight: 600; }
.scan-all-btn { margin-left: auto; }
.scan-progress-wrap { padding: 6px 0 10px; }
.scan-progress-label { font-size: 12px; color: var(--text-3); margin-bottom: 5px; display: flex; justify-content: space-between; align-items: center; }
.scan-pct { color: var(--accent); font-weight: 600; font-variant-numeric: tabular-nums; }
.scan-bar { height: 6px; background: var(--surface-2); border-radius: 3px; overflow: hidden; margin-bottom: 3px; }
.scan-bar-file { height: 4px; }
.scan-bar-fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width .2s ease; }
.scan-bar-fill.file { background: var(--accent); opacity: .55; }

/* 正在扫描的版本行：呼吸灯边框 */
@keyframes scan-breathe {
  0%, 100% { border-color: var(--accent); box-shadow: 0 0 0 0 rgba(79, 110, 247, 0); }
  50% { border-color: var(--accent); box-shadow: 0 0 0 3px rgba(79, 110, 247, .20); }
}
.ver-item.item-scanning { animation: scan-breathe 1.4s ease-in-out infinite; }

/* 刚扫完的版本行：短暂闪烁 */
@keyframes scan-flash {
  0% { background: rgba(34, 197, 94, .22); }
  100% { background: transparent; }
}
.ver-item.item-flash { animation: scan-flash .6s ease-out; }
/* .btn / .badge 等公共样式见 src/styles/common.css（main.js 引入），此处不再重复 */
</style>
