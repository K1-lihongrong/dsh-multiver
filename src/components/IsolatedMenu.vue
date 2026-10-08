<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useToast } from "../composables/useToast.js";
import { t } from "../composables/useI18n.js";

// 隔离管理下拉菜单（仅"已隔离"版本显示）。
// 自包含：自己管菜单开关、隔离数据大小扫描。
// 需要父组件参与的操作（关闭隔离 / 开启隔离）通过事件抛出（父组件要刷新列表）。
const props = defineProps({
  version: { type: String, required: true },
  disabled: { type: Boolean, default: false },
});
const emit = defineEmits(["toggle-isolated"]);

const { notify } = useToast();

const open = ref(false);
const scanning = ref(false);
const size = ref(undefined); // 隔离数据字节数（undefined=未扫描）

function toggle() {
  if (props.disabled) return;
  open.value = !open.value;
}

/// 点菜单项前先收起
function close() {
  open.value = false;
}

function fmtSize(bytes) {
  if (bytes == null) return "";
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  if (bytes < 1024 * 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
  return (bytes / 1024 / 1024 / 1024).toFixed(2) + " GB";
}

async function scanSize() {
  scanning.value = true;
  try {
    size.value = await invoke("scan_isolated_size", { version: props.version });
  } catch (e) {
    notify("" + e);
  } finally {
    scanning.value = false;
  }
}

async function openDir() {
  close();
  try { await invoke("open_isolated_dir", { version: props.version }); }
  catch (e) { notify("" + e); }
}

async function copyShared() {
  close();
  if (!(await ask(t("iso.copySharedConfirm"), { title: t("iso.copyShared"), kind: "warning" }))) return;
  try {
    notify(await invoke("copy_shared_to_isolated", { version: props.version }));
    size.value = undefined;
  } catch (e) { notify("" + e); }
}

async function clearIsolated() {
  close();
  if (!(await ask(t("iso.clearConfirm"), { title: t("iso.clear"), kind: "warning" }))) return;
  try {
    notify(await invoke("clear_isolated_data", { version: props.version }));
    size.value = 0;
  } catch (e) { notify("" + e); }
}

function closeIsolated() {
  close();
  emit("toggle-isolated", true); // true = 当前是隔离，请求关闭
}

// 点击组件外部时收起菜单（组件根元素上的 @click.stop 阻止了内部点击冒泡到 document）
function onDocClick() {
  if (open.value) open.value = false;
}
onMounted(() => document.addEventListener("click", onDocClick));
onUnmounted(() => document.removeEventListener("click", onDocClick));
</script>

<template>
  <div class="menu-wrap" @click.stop>
    <button class="btn active" @click.stop="toggle" :disabled="disabled">
      {{ t("installed.isolatedMenu") }} ▾
    </button>
    <div class="menu" v-if="open" @click.stop>
      <button class="menu-item" @click="closeIsolated">{{ t("iso.close") }}</button>
      <button class="menu-item" @click="openDir">{{ t("iso.openDir") }}</button>
      <button class="menu-item" @click="scanSize" :disabled="scanning">
        <span v-if="scanning">{{ t("installed.scanning") }}</span>
        <span v-else-if="size != null">{{ t("installed.usage") }} {{ fmtSize(size) }}（{{ t("iso.rescan") }}）</span>
        <span v-else>{{ t("iso.scanSize") }}</span>
      </button>
      <button class="menu-item" @click="copyShared">{{ t("iso.copyShared") }}</button>
      <button class="menu-item danger" @click="clearIsolated">{{ t("iso.clear") }}</button>
    </div>
  </div>
</template>

<style scoped>
.menu-wrap { position: relative; }
.menu {
  position: absolute; top: calc(100% + 4px); right: 0; z-index: 50;
  background: var(--panel-bg); border: 1px solid var(--surface-5); border-radius: 8px;
  box-shadow: 0 6px 20px var(--shadow-pop); min-width: 200px;
  padding: 4px; display: flex; flex-direction: column;
}
.menu-item {
  text-align: left; border: none; background: none; color: var(--text-2);
  padding: 8px 12px; border-radius: 6px; font-size: 13px; cursor: pointer;
  font-family: inherit; transition: background .12s;
}
.menu-item:hover:not(:disabled) { background: var(--surface-4); }
.menu-item:disabled { opacity: .6; cursor: not-allowed; }
.menu-item.danger { color: var(--danger); }
.menu-item.danger:hover { background: var(--danger-weak); }
</style>
