<script setup>
import { ref, computed, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";

// v0.3.0 双向桥 B1：管理某 dsh 版本的插件（dsh profile 里的 npm 包）。
// 通过 dsh CLI（dsh plugin --profile <p> add/remove <包>）执行。
const props = defineProps({
  version: { type: String, required: true },
});
const emit = defineEmits(["close"]);

const { notify } = useToast();
const profiles = ref([]);
const dshHome = ref("");
const homeKind = ref("shared");
const loading = ref(false);
const busy = ref(false);
const activeProfile = ref("");
const newPkg = ref("");
const activeTab = ref("plugins"); // plugins | logs? (预留)

const homeKindLabel = computed(() => {
  const m = { isolated: "dsplugins.homeIsolated", official: "dsplugins.homeOfficial", shared: "dsplugins.homeShared" };
  return t(m[homeKind.value] || "dsplugins.homeShared");
});
const homeKindClass = computed(() => "kind-" + homeKind.value);

async function load() {
  loading.value = true;
  try {
    const res = await invoke("ds_list_plugins", { version: props.version });
    profiles.value = res?.profiles || [];
    dshHome.value = res?.dsh_home || "";
    homeKind.value = res?.home_kind || "shared";
    if (!activeProfile.value && profiles.value.length) {
      activeProfile.value = profiles.value[0].name;
    }
  } catch (e) {
    notify("" + e);
  } finally {
    loading.value = false;
  }
}

const currentProfile = () => profiles.value.find((p) => p.name === activeProfile.value);

async function doAction(action, pkg) {
  const name = (pkg || "").trim();
  if (!name) return;
  busy.value = true;
  try {
    const out = await invoke("ds_plugin_action", {
      version: props.version,
      profile: activeProfile.value,
      action,
      package: name,
    });
    notify(out || t(action === "add" ? "dsplugins.installed" : "dsplugins.removed", { p: name }));
    newPkg.value = "";
    await load();
  } catch (e) {
    notify("" + e);
  } finally {
    busy.value = false;
  }
}

async function openDir() {
  try {
    await invoke("ds_open_profile_dir", { version: props.version, profile: activeProfile.value });
  } catch (e) {
    notify("" + e);
  }
}

onMounted(load);
</script>

<template>
  <div class="ds-plugin-panel">
    <!-- 头部：标题 + DSH_HOME + 操作 -->
    <div class="dp-head">
      <div class="dp-head-left">
        <span class="dp-title">{{ t("dsplugins.title", { v: version }) }}</span>
        <span class="dp-home" :class="homeKindClass" :title="dshHome">
          <span class="dp-home-kind">{{ homeKindLabel }}</span>
          <code class="dp-home-path">{{ dshHome }}</code>
        </span>
      </div>
      <div class="dp-head-actions">
        <button class="dp-icon-btn" @click="load" :disabled="loading" :title="t('dsplugins.refresh')">↻</button>
        <button class="dp-icon-btn" @click="openDir" :disabled="!activeProfile" :title="t('dsplugins.openDir')">📂</button>
        <button class="dp-icon-btn" @click="emit('close')" :title="t('dsplugins.close')">✕</button>
      </div>
    </div>

    <div class="dp-hint">{{ t("dsplugins.hint") }}</div>

    <!-- 加载 / 空 / 内容 -->
    <div v-if="loading" class="dp-state">
      <span class="dp-spinner"></span>
      <span>{{ t("dsplugins.loading") }}</span>
    </div>
    <div v-else-if="!profiles.length" class="dp-state dp-state-empty">
      <span class="dp-state-icon">📦</span>
      <span>{{ t("dsplugins.noProfile") }}</span>
    </div>
    <template v-else>
      <!-- profile 切换（多个时） -->
      <div class="dp-toolbar">
        <select v-if="profiles.length > 1" class="dp-select" :value="activeProfile" @change="activeProfile = $event.target.value">
          <option v-for="p in profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
        <span v-else class="dp-profile-tag">{{ activeProfile }}</span>

        <div class="dp-add">
          <input
            class="dp-input"
            v-model="newPkg"
            :placeholder="t('dsplugins.addPlaceholder')"
            @keyup.enter="doAction('add', newPkg)"
            :disabled="busy"
          />
          <button class="dp-btn dp-btn-primary" @click="doAction('add', newPkg)" :disabled="busy || !newPkg.trim()">
            {{ busy ? "…" : t("dsplugins.add") }}
          </button>
        </div>
      </div>

      <!-- 插件列表 -->
      <div v-if="!currentProfile() || !currentProfile().plugins.length" class="dp-state dp-state-empty">
        <span class="dp-state-icon">🧩</span>
        <span>{{ t("dsplugins.empty") }}</span>
      </div>
      <ul class="dp-list" v-else>
        <li v-for="pl in currentProfile().plugins" :key="pl.name" class="dp-item">
          <div class="dp-item-main">
            <span class="dp-item-icon">{{ pl.is_bundle ? "📦" : "🧩" }}</span>
            <div class="dp-item-info">
              <code class="dp-item-name">{{ pl.name }}</code>
              <span class="dp-item-ver" v-if="pl.version">v{{ pl.version }}</span>
            </div>
            <span class="dp-badge" v-if="pl.is_bundle">{{ t("dsplugins.bundle") }}</span>
          </div>
          <button class="dp-btn dp-btn-danger dp-btn-sm" @click="doAction('remove', pl.name)" :disabled="busy">
            {{ t("dsplugins.remove") }}
          </button>
        </li>
      </ul>
    </template>
  </div>
</template>

<style scoped>
.ds-plugin-panel {
  flex: 1 0 100%;
  width: 100%;
  box-sizing: border-box;
  margin: 10px 0 4px 0;
  padding: 0;
  background: var(--panel-bg);
  border: 1px solid var(--panel-border);
  border-radius: 10px;
  overflow: hidden;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);
}

/* 头部 */
.dp-head {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 12px 16px;
  background: linear-gradient(180deg, var(--surface-2), var(--panel-bg));
  border-bottom: 1px solid var(--panel-border);
}
.dp-head-left { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; }
.dp-title { font-size: 13px; font-weight: 600; color: var(--text); }
.dp-home {
  display: inline-flex; align-items: center; gap: 6px;
  font-size: 11px; padding: 2px 8px; border-radius: 12px;
  border: 1px solid var(--panel-border); background: var(--surface-3);
  max-width: 360px; overflow: hidden;
}
.dp-home-kind { font-weight: 600; white-space: nowrap; }
.dp-home-path {
  color: var(--text-muted); font-size: 10.5px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.dp-home.kind-official { border-color: #f5c542; background: rgba(245,197,66,0.12); }
.dp-home.kind-official .dp-home-kind { color: #b8860b; }
.dp-home.kind-isolated { border-color: #30a46c; background: rgba(48,164,108,0.1); }
.dp-home.kind-isolated .dp-home-kind { color: #30a46c; }
.dp-home.kind-shared .dp-home-kind { color: var(--text-3); }

.dp-head-actions { display: flex; gap: 4px; flex: none; }
.dp-icon-btn {
  width: 26px; height: 26px; border-radius: 6px; cursor: pointer;
  border: 1px solid var(--panel-border); background: var(--panel-bg);
  color: var(--text-3); font-size: 13px; line-height: 1;
  display: inline-flex; align-items: center; justify-content: center;
  transition: all .15s;
}
.dp-icon-btn:hover:not(:disabled) { background: var(--surface-3); color: var(--text); border-color: var(--accent-border); }
.dp-icon-btn:disabled { opacity: .4; cursor: not-allowed; }

.dp-hint {
  padding: 8px 16px; font-size: 11px; color: var(--text-muted);
  border-bottom: 1px dashed var(--panel-border);
  background: var(--surface-2);
}

/* 状态（加载/空） */
.dp-state {
  display: flex; align-items: center; justify-content: center; gap: 8px;
  padding: 24px 16px; font-size: 12px; color: var(--text-muted);
}
.dp-state-icon { font-size: 18px; opacity: .6; }
.dp-spinner {
  width: 14px; height: 14px; border-radius: 50%;
  border: 2px solid var(--surface-3); border-top-color: var(--accent);
  animation: dp-spin .7s linear infinite;
}
@keyframes dp-spin { to { transform: rotate(360deg); } }

/* 工具栏 */
.dp-toolbar {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  padding: 10px 16px;
}
.dp-select, .dp-input {
  font-size: 12px; padding: 5px 10px; border-radius: 6px;
  border: 1px solid var(--panel-border); background: var(--panel-bg);
  color: var(--text); outline: none;
}
.dp-select:focus, .dp-input:focus { border-color: var(--accent-border); }
.dp-profile-tag {
  font-size: 11px; font-weight: 600; padding: 3px 10px; border-radius: 12px;
  background: var(--accent-weak); color: var(--accent); border: 1px solid var(--accent-border);
}
.dp-add { display: flex; gap: 6px; flex: 1; min-width: 240px; }
.dp-add .dp-input { flex: 1; }

/* 按钮 */
.dp-btn {
  font-size: 12px; padding: 5px 14px; border-radius: 6px; cursor: pointer;
  border: 1px solid var(--panel-border); background: var(--panel-bg); color: var(--text);
  transition: all .15s; white-space: nowrap;
}
.dp-btn:hover:not(:disabled) { background: var(--surface-3); }
.dp-btn:disabled { opacity: .5; cursor: not-allowed; }
.dp-btn-primary {
  background: var(--accent); color: #fff; border-color: var(--accent);
}
.dp-btn-primary:hover:not(:disabled) { filter: brightness(1.08); }
.dp-btn-danger { color: var(--danger, #e5484d); border-color: var(--danger, #e5484d); }
.dp-btn-danger:hover:not(:disabled) { background: rgba(229,72,77,0.08); }
.dp-btn-sm { font-size: 11px; padding: 3px 10px; }

/* 插件列表：自适应列数（宽屏多列，窄屏单列） */
.dp-list {
  list-style: none; margin: 0; padding: 6px 16px 14px;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 8px;
}
.dp-item {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 8px 12px; border-radius: 7px;
  background: var(--surface-2); border: 1px solid transparent;
  transition: all .12s;
  min-width: 0;
}
.dp-item:hover { border-color: var(--panel-border); background: var(--surface-3); }
.dp-item-main { display: flex; align-items: center; gap: 8px; min-width: 0; flex: 1; }
.dp-item-icon { font-size: 14px; flex: none; opacity: .8; }
.dp-item-info { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
.dp-item-name {
  font-size: 12px; color: var(--text); font-family: ui-monospace, Consolas, monospace;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.dp-item-ver { font-size: 10.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; flex: none; }
.dp-badge {
  font-size: 9.5px; padding: 1px 7px; border-radius: 8px; flex: none;
  background: var(--accent-weak); color: var(--accent); border: 1px solid var(--accent-border);
  font-weight: 500;
}
</style>
