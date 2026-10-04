<script setup>
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useToast } from "../composables/useToast.js";
import { useSharedHomeWarn } from "../composables/useSharedHomeWarn.js";
import IsolatedMenu from "./IsolatedMenu.vue";
import NotesPanel from "./NotesPanel.vue";

// 已安装版本列表（含批量操作、占用扫描、隔离、更新说明）。
// installed 由父组件传入；动作自包含（自己 invoke），完成后 emit refresh 让父组件重拉列表。
const props = defineProps({
  installed: { type: Array, default: () => [] },
});
const emit = defineEmits(["refresh", "open-url"]);

const { notify } = useToast();
const { warnSharedHomeEnabled } = useSharedHomeWarn();

const runningVersion = ref(""); // 正在启动的版本号
const uninstalling = ref("");   // 正在卸载的版本号
const scanningVer = ref(null);  // 正在扫描"整版本占用"的版本号
const verSizes = ref({});       // 版本号 -> { total, shared_size, shared_count, exclusive_size }
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
  scanningVer.value = v;
  try {
    const info = await invoke("scan_version_size", { version: v });
    verSizes.value = { ...verSizes.value, [v]: info };
  } catch (e) { notify("" + e); }
  finally { scanningVer.value = null; }
}

/// 运行非隔离（共享 home）版本前的提示。返回 false 表示用户取消。
async function confirmSharedHome(v) {
  if (!warnSharedHomeEnabled.value) return true;
  const ver = props.installed.find((x) => x.version === v);
  if (!ver || ver.isolated) return true; // 隔离版本无此风险
  return await ask(
    "版本 " + v + " 使用【共享 home】（<根>/home）。\n\n" +
    "多个版本共用同一个 home 时，插件/依赖可能相互影响，\n" +
    "导致某些功能异常（例如 0.1.7 的「在文件管理器中打开」失效会连累其它版本）。\n\n" +
    "建议对测试版本开启【数据隔离】。\n\n" +
    "（可在「路径设置」里关闭此提示）\n\n" +
    "继续运行吗？",
    { title: "共享 home 提示", kind: "warning" }
  );
}

async function run(v) {
  if (!(await confirmSharedHome(v))) return;
  if (runningVersion.value) return;
  runningVersion.value = v;
  notify("正在启动 DSH " + v + "，请稍候...");
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
  try { notify(await invoke("set_default", { version: v })); emit("refresh"); }
  catch (e) { notify("" + e); }
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
  if (!(await ask("确定卸载版本 " + v + " ?", { title: "卸载版本", kind: "warning" }))) return;
  uninstalling.value = v;
  notify("正在卸载 " + v + "...");
  try {
    notify(await invoke("uninstall_version", { version: v }));
    emit("refresh");
  } catch (e) { notify("" + e); }
  finally { uninstalling.value = ""; }
}

/// 批量卸载已选版本
async function batchUninstall() {
  const names = Array.from(selected.value);
  if (!names.length) return notify("未选择任何项");
  if (!(await ask("确定卸载选中的 " + names.length + " 个版本？此操作不可恢复。", { title: "批量卸载", kind: "warning" }))) return;
  batchBusy.value = true;
  let okCount = 0;
  const failed = [];
  for (let i = 0; i < names.length; i++) {
    const name = names[i];
    notify("正在卸载（" + (i + 1) + "/" + names.length + "）：" + name);
    uninstalling.value = name;
    try { await invoke("uninstall_version", { version: name }); okCount++; }
    catch (e) { failed.push(name + "：" + e); }
  }
  emit("refresh");
  uninstalling.value = "";
  selected.value = new Set();
  batchBusy.value = false;
  if (failed.length) {
    notify("批量卸载完成：成功 " + okCount + " 个，失败 " + failed.length + " 个。\n失败详情：\n" + failed.join("\n"));
  } else {
    notify("批量卸载完成：共 " + okCount + " 个");
  }
}

function toggleInstalledNote(v) {
  installedNoteVer.value = installedNoteVer.value === v ? null : v;
}
</script>

<template>
  <section class="panel">
    <div class="panel-head">
      <h2>已安装版本</h2>
      <span class="count" v-if="installed.length">{{ installed.length }}</span>
    </div>

    <div v-if="!installed.length" class="empty">
      还没有安装任何版本，从下方列表安装一个吧
    </div>

    <ul class="ver-list" v-else>
      <li v-for="v in installed" :key="v.version" class="ver-item" :class="{ 'item-selected': isSelected(v.version) }">
        <input type="checkbox" class="sel-box" :checked="isSelected(v.version)" :disabled="batchBusy" @change="toggleSelect(v.version)" />
        <div class="ver-main">
          <span class="ver-num ver-num-clickable" title="点击查看该版本更新说明" @click="toggleInstalledNote(v.version)">
            {{ v.version }}<span class="ver-caret">{{ installedNoteVer === v.version ? "▾" : "▸" }}</span>
          </span>
          <span class="badge" v-if="v.is_default">默认</span>
          <span class="badge badge-iso" v-if="v.isolated">隔离</span>
          <span class="badge badge-shared" v-else title="使用共享 home：多版本混用可能导致插件/依赖版本错配，测试版建议开隔离">共享 home</span>
          <span class="ver-meta" v-if="v.installed_at">安装于 {{ v.installed_at }}</span>
          <span class="ver-meta" v-if="verSizes[v.version] != null">
            占用 {{ fmtSize(verSizes[v.version].total) }}
            <template v-if="verSizes[v.version].shared_count">
              （复用 {{ fmtSize(verSizes[v.version].shared_size) }} / 独占 {{ fmtSize(verSizes[v.version].exclusive_size) }}）
            </template>
          </span>
        </div>
        <div class="ver-actions">
          <button class="btn primary" @click="run(v.version)" :disabled="!!runningVersion || isBlocked(v.version)">{{ runningVersion === v.version ? "启动中..." : "运行" }}</button>
          <button class="btn" @click="openInBrowser(v.version)" :disabled="isBlocked(v.version)" title="在新终端启动并在系统浏览器打开">浏览器打开</button>
          <button class="btn" @click="setDefault(v.version)" :disabled="v.is_default || isBlocked(v.version)">设为默认</button>
          <button class="btn" @click="createShortcut(v.version)" :disabled="isBlocked(v.version)" title="在桌面创建 DSH 快捷方式">桌面快捷方式</button>
          <button class="btn" @click="scanVersionSize(v.version)" :disabled="scanningVer === v.version || isBlocked(v.version)" title="统计该版本占用的磁盘空间">
            {{ scanningVer === v.version ? "扫描中..." : (verSizes[v.version] != null ? "重新扫描占用" : "扫描占用") }}
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
            title="开启隔离（该版本使用独立数据目录）"
          >隔离</button>
          <button class="btn danger" @click="uninstall(v.version)" :disabled="!!uninstalling || batchBusy">{{ uninstalling === v.version ? "卸载中..." : "卸载" }}</button>
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
        全选
      </label>
      <template v-if="selected.size">
        <span class="batch-count">已选 {{ selected.size }} 项</span>
        <button class="btn small" @click="clearSelection" :disabled="batchBusy">清除选择</button>
        <button class="btn danger small" @click="batchUninstall" :disabled="batchBusy">
          {{ batchBusy ? "批量卸载中..." : "批量卸载" }}
        </button>
      </template>
    </div>
  </section>
</template>

<style scoped>
.count {
  background: #eef1f5; color: #6b7280; font-size: 12px; font-weight: 600;
  padding: 1px 8px; border-radius: 10px; margin-left: 8px;
}
.empty { color: #9aa1ab; font-size: 13px; padding: 8px 0; }
.ver-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.ver-item {
  display: flex; align-items: center; flex-wrap: wrap;
  gap: 6px 12px;
  padding: 10px 14px; border: 1px solid #eceef1; border-radius: 9px;
  background: #fcfcfd; transition: border-color .15s, background .15s;
}
.ver-item:hover { border-color: #d6dae0; background: #fff; }
.ver-item.item-selected { border-color: #4f6ef7; background: #f5f7ff; }
.sel-box { flex-shrink: 0; cursor: pointer; }
.ver-main { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; flex: 1 1 auto; }
.ver-num { font-weight: 600; font-size: 14px; font-variant-numeric: tabular-nums; }
.ver-num-clickable { cursor: pointer; user-select: none; }
.ver-num-clickable:hover { color: #4f6ef7; }
.ver-caret { font-size: 10px; margin-left: 3px; color: #9aa1ab; }
.ver-meta { color: #9aa1ab; font-size: 11px; }
.ver-actions { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; justify-content: flex-start; }
.batch-bar {
  display: flex; align-items: center; gap: 12px; flex-wrap: wrap;
  margin-top: 12px; padding-top: 12px; border-top: 1px solid #eef1f5;
}
.batch-select-all { display: flex; align-items: center; gap: 6px; font-size: 13px; color: #6b7280; cursor: pointer; }
.batch-count { font-size: 13px; color: #4f6ef7; font-weight: 600; }
/* .btn / .badge 等公共样式见 src/styles/common.css（main.js 引入），此处不再重复 */
</style>
