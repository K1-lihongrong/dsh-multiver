<script setup>
import { ref, onMounted } from "vue";
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
const loading = ref(false);
const busy = ref(false);
const activeProfile = ref("");
const newPkg = ref("");

async function load() {
  loading.value = true;
  try {
    profiles.value = (await invoke("ds_list_plugins", { version: props.version })) || [];
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
    <div class="dp-head">
      <span class="dp-title">{{ t("dsplugins.title", { v: version }) }}</span>
      <div class="dp-head-actions">
        <button class="btn small" @click="openDir" :disabled="!activeProfile">{{ t("dsplugins.openDir") }}</button>
        <button class="btn small" @click="emit('close')">{{ t("dsplugins.close") }}</button>
      </div>
    </div>

    <div class="dp-hint">{{ t("dsplugins.hint") }}</div>

    <div v-if="loading" class="dp-empty">{{ t("dsplugins.loading") }}</div>
    <div v-else-if="!profiles.length" class="dp-empty">{{ t("dsplugins.noProfile") }}</div>
    <template v-else>
      <div class="dp-profile-row" v-if="profiles.length > 1">
        <label>{{ t("dsplugins.profile") }}</label>
        <select class="input" :value="activeProfile" @change="activeProfile = $event.target.value">
          <option v-for="p in profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
      </div>

      <div class="dp-add-row">
        <input
          class="input"
          v-model="newPkg"
          :placeholder="t('dsplugins.addPlaceholder')"
          @keyup.enter="doAction('add', newPkg)"
          :disabled="busy"
        />
        <button class="btn small primary" @click="doAction('add', newPkg)" :disabled="busy || !newPkg.trim()">
          {{ t("dsplugins.add") }}
        </button>
      </div>

      <div v-if="!currentProfile() || !currentProfile().plugins.length" class="dp-empty">
        {{ t("dsplugins.empty") }}
      </div>
      <ul class="dp-list" v-else>
        <li v-for="pl in currentProfile().plugins" :key="pl.name" class="dp-item">
          <div class="dp-name">
            <code>{{ pl.name }}</code>
            <span class="dp-ver">{{ pl.version }}</span>
            <span class="dp-badge" v-if="pl.is_bundle">{{ t("dsplugins.bundle") }}</span>
          </div>
          <button class="btn small danger" @click="doAction('remove', pl.name)" :disabled="busy">
            {{ t("dsplugins.remove") }}
          </button>
        </li>
      </ul>
    </template>
  </div>
</template>

<style scoped>
.ds-plugin-panel {
  margin: 8px 0 4px 26px; padding: 12px 14px;
  background: var(--surface-2); border: 1px solid var(--panel-border);
  border-radius: 8px;
}
.dp-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.dp-title { font-size: 13px; font-weight: 600; }
.dp-head-actions { display: flex; gap: 6px; }
.dp-hint { font-size: 11px; color: var(--text-muted); margin: 6px 0 10px; }
.dp-empty { font-size: 12px; color: var(--text-muted); padding: 10px 0; }
.dp-profile-row, .dp-add-row { display: flex; gap: 8px; align-items: center; margin-bottom: 8px; }
.dp-profile-row label { font-size: 12px; color: var(--text-3); min-width: 60px; }
.dp-add-row .input { flex: 1; }
.dp-list { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.dp-item {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 6px 10px; background: var(--panel-bg); border: 1px solid var(--panel-border); border-radius: 6px;
}
.dp-name { display: flex; align-items: center; gap: 8px; min-width: 0; }
.dp-name code { font-size: 12px; overflow: hidden; text-overflow: ellipsis; }
.dp-ver { font-size: 11px; color: var(--text-muted); }
.dp-badge {
  font-size: 10px; padding: 1px 6px; border-radius: 8px;
  background: var(--accent-weak); color: var(--accent); border: 1px solid var(--accent-border);
}
.btn.small { padding: 3px 10px; font-size: 12px; }
.btn.small.danger { color: var(--danger, #e5484d); border-color: var(--danger, #e5484d); }
</style>
