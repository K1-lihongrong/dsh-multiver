<script setup>
import { computed, ref } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import { useToast } from "../composables/useToast.js";
import { t } from "../composables/useI18n.js";
import { runVersion } from "../api/versions.js";
import {
  createInstance,
  renameInstance,
  removeInstance,
  attachInstance,
  detachInstance,
  scanInstanceSize,
  openInstanceDir,
} from "../api/instances.js";

const props = defineProps({
  instances: { type: Array, default: () => [] },
  versionTargets: { type: Object, default: () => ({}) },
  runningTargets: { type: Array, default: () => [] },
  installed: { type: Array, default: () => [] },
});
const emit = defineEmits(["close", "changed"]);
const { notify } = useToast();

const ordered = computed(() =>
  [...props.instances].sort((a, b) => {
    if (a.orphan !== b.orphan) return a.orphan ? -1 : 1;
    return a.id.localeCompare(b.id);
  })
);
const orphanCount = computed(() => props.instances.filter((i) => i.orphan).length);

const sizes = ref({});
const runPick = ref({});
const editingId = ref("");
const draft = ref("");
const creating = ref(false);
const newName = ref("");
const newVersions = ref([]);
const creatingBusy = ref(false);

function runningOn(inst) {
  const hit = props.runningTargets.find(
    (x) => x.kind === "instance" && x.instance === inst.id
  );
  return hit ? hit.version : inst.running_on || "";
}
function sizeText(inst) {
  const n = sizes.value[inst.id];
  if (n == null) return "";
  const mb = n / 1024 / 1024;
  return mb >= 1024 ? (mb / 1024).toFixed(2) + " GB" : mb.toFixed(1) + " MB";
}
async function scan(inst) {
  try {
    sizes.value = { ...sizes.value, [inst.id]: await scanInstanceSize(inst.id) };
  } catch (e) { notify("" + e); }
}
async function run(inst) {
  if (!inst.mounted_versions.length) {
    notify(t("instances.unmounted"));
    return;
  }
  const version = runPick.value[inst.id] || inst.mounted_versions[0];
  try {
    notify(await runVersion(version, inst.id));
  } catch (e) { notify("" + e); }
}
async function openDir(inst) {
  try { await openInstanceDir(inst.id); }
  catch (e) { notify("" + e); }
}
function startRename(inst) {
  editingId.value = inst.id;
  draft.value = inst.name;
}
async function saveRename(inst) {
  const name = draft.value.trim();
  if (!name) return;
  try {
    await renameInstance(inst.id, name);
    editingId.value = "";
    emit("changed");
  } catch (e) { notify("" + e); }
}
async function remove(inst) {
  if (runningOn(inst)) {
    notify(t("instances.removeRunning"));
    return;
  }
  if (!(await ask(t("instances.removeConfirm", { name: inst.name }), {
    title: t("instances.remove"),
    kind: "warning",
  }))) return;
  try {
    await removeInstance(inst.id);
    emit("changed");
  } catch (e) { notify("" + e); }
}
async function toggleAttach(inst, version, checked) {
  try {
    if (checked) await attachInstance(version, inst.id);
    else await detachInstance(version, inst.id);
    emit("changed");
  } catch (e) { notify("" + e); }
}
function startCreate() {
  creating.value = true;
  newName.value = "";
  newVersions.value = [];
}
async function submitCreate() {
  const name = newName.value.trim();
  if (!name) return;
  creatingBusy.value = true;
  try {
    await createInstance(name, newVersions.value);
    creating.value = false;
    emit("changed");
  } catch (e) { notify("" + e); }
  finally { creatingBusy.value = false; }
}
</script>

<template>
  <div class="ip-mask" @click.self="$emit('close')">
    <div class="ip-modal">
      <div class="ip-head">
        <h3>{{ t("instances.panelTitle") }}</h3>
        <span class="ip-orphans" v-if="orphanCount">
          {{ t("instances.orphanCount", { n: orphanCount }) }}
        </span>
        <button class="ip-x" @click="$emit('close')">×</button>
      </div>

      <div class="ip-create">
        <button v-if="!creating" class="btn primary small" @click="startCreate">
          {{ t("instances.new") }}
        </button>
        <div v-else class="ip-create-form">
          <input
            v-model="newName"
            class="ip-input"
            :placeholder="t('instances.namePlaceholder')"
            maxlength="32"
            @keyup.enter="submitCreate"
          />
          <div class="ip-versions">
            <label v-for="v in installed" :key="v.version">
              <input
                type="checkbox"
                :value="v.version"
                v-model="newVersions"
              />
              {{ v.version }}
            </label>
          </div>
          <button class="btn small" @click="creating = false">{{ t("instances.cancel") }}</button>
          <button class="btn primary small" :disabled="creatingBusy || !newName.trim()" @click="submitCreate">
            {{ t("instances.create") }}
          </button>
        </div>
      </div>

      <div class="ip-list">
        <div v-if="!ordered.length" class="ip-empty">{{ t("instances.empty") }}</div>
        <div v-for="inst in ordered" :key="inst.id" class="ip-row" :class="{ orphan: inst.orphan }">
          <div class="ip-main">
            <template v-if="editingId === inst.id">
              <input v-model="draft" class="ip-input" maxlength="32" @keyup.enter="saveRename(inst)" />
              <button class="btn small primary" @click="saveRename(inst)">{{ t("instances.save") }}</button>
              <button class="btn small" @click="editingId = ''">{{ t("instances.cancel") }}</button>
            </template>
            <template v-else>
              <span class="ip-name">{{ inst.name }}</span>
              <span class="ip-id">{{ inst.id }}</span>
              <span v-if="inst.orphan" class="ip-badge">{{ t("instances.unmounted") }}</span>
              <span v-if="runningOn(inst)" class="ip-running">{{ t("instances.runningOn", { v: runningOn(inst) }) }}</span>
              <button class="ip-dir" :title="inst.home" @click="openDir(inst)">{{ t("instances.openDir") }}</button>
            </template>
          </div>

          <div class="ip-meta">
            <span class="ip-home" :title="inst.home">{{ inst.home }}</span>
            <span v-if="sizeText(inst)" class="ip-size">{{ t("instances.size") }} {{ sizeText(inst) }}</span>
            <button class="ip-link" @click="scan(inst)">{{ t("instances.scan") }}</button>
          </div>

          <div class="ip-targets">
            <span class="ip-label">{{ t("instances.target") }}:</span>
            <span class="ip-chip" :class="{ on: versionTargets && versionTargets[inst.mounted_versions?.[0]] === inst.id }">
              {{ Object.values(versionTargets || {}).includes(inst.id) ? "✓" : "—" }}
            </span>
            <span class="ip-label">{{ t("instances.attach") }}:</span>
            <label v-for="v in installed" :key="v.version" class="ip-check">
              <input
                type="checkbox"
                :checked="(inst.mounted_versions || []).includes(v.version)"
                @change="toggleAttach(inst, v.version, $event.target.checked)"
              />
              {{ v.version }}
            </label>
          </div>

          <div class="ip-actions">
            <select
              v-if="inst.mounted_versions.length > 1"
              v-model="runPick[inst.id]"
              class="ip-select"
            >
              <option v-for="v in inst.mounted_versions" :key="v" :value="v">{{ v }}</option>
            </select>
            <button class="btn small primary ip-run" @click="run(inst)">
              {{ runningOn(inst) ? t("instances.focus") : t("instances.run") }}
            </button>
            <button class="btn small" @click="startRename(inst)">{{ t("instances.rename") }}</button>
            <button
              class="btn small danger ip-delete"
              :disabled="!!runningOn(inst)"
              :title="runningOn(inst) ? t('instances.removeRunning') : ''"
              @click="remove(inst)"
            >{{ t("instances.remove") }}</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ip-mask {
  position: fixed; inset: 0; z-index: 1200; background: var(--shadow-strong);
  display: flex; align-items: center; justify-content: center;
}
.ip-modal {
  width: 820px; max-width: 94vw; max-height: 82vh; overflow: auto;
  background: var(--panel-bg); border-radius: 12px; padding: 16px 18px;
  box-shadow: 0 16px 48px var(--overlay);
}
.ip-head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; }
.ip-head h3 { margin: 0; font-size: 15px; }
.ip-orphans {
  background: var(--warn-weak); color: var(--warn); border: 1px solid var(--warn-border);
  border-radius: 9px; padding: 1px 8px; font-size: 11px;
}
.ip-x {
  margin-left: auto; border: none; background: none; color: var(--text-muted);
  font-size: 20px; cursor: pointer; line-height: 1;
}
.ip-x:hover { color: var(--text); }
.ip-create { margin-bottom: 10px; }
.ip-create-form { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.ip-input {
  padding: 6px 9px; border: 1px solid var(--border); border-radius: 6px;
  font-size: 13px; color: var(--text); background: var(--surface-1); min-width: 180px;
}
.ip-versions { display: flex; gap: 8px; flex-wrap: wrap; font-size: 12px; color: var(--text-2); }
.ip-list { display: flex; flex-direction: column; gap: 8px; }
.ip-empty { color: var(--text-muted); font-size: 13px; padding: 10px 0; }
.ip-row {
  border: 1px solid var(--surface-5); border-radius: 9px; padding: 9px 11px;
  display: flex; flex-direction: column; gap: 6px; background: var(--panel-bg);
}
.ip-row.orphan { border-color: var(--warn-border); background: var(--warn-weak); }
.ip-main { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.ip-name { font-size: 14px; font-weight: 600; color: var(--text); }
.ip-id { font-size: 11px; color: var(--text-muted); font-family: ui-monospace, Consolas, monospace; }
.ip-badge {
  background: var(--warn-weak-2); color: var(--warn); border-radius: 9px;
  padding: 1px 7px; font-size: 11px;
}
.ip-running { color: var(--ok); font-size: 12px; }
.ip-dir {
  border: none; background: none; color: var(--accent); font-size: 11px;
  cursor: pointer; padding: 0; font-family: inherit;
}
.ip-meta { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.ip-home {
  font-size: 11px; color: var(--text-muted); max-width: 420px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.ip-size { font-size: 11px; color: var(--text-3); }
.ip-link {
  border: none; background: none; color: var(--accent); font-size: 11px;
  cursor: pointer; padding: 0; font-family: inherit;
}
.ip-targets { display: flex; align-items: center; gap: 6px 10px; flex-wrap: wrap; font-size: 11px; }
.ip-label { color: var(--text-muted); }
.ip-chip {
  border: 1px solid var(--border); border-radius: 6px; padding: 0 5px;
  color: var(--text-muted); font-size: 11px;
}
.ip-chip.on { color: var(--ok); border-color: var(--ok); }
.ip-check { display: inline-flex; align-items: center; gap: 3px; color: var(--text-2); }
.ip-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.ip-select {
  padding: 3px 6px; border: 1px solid var(--border); border-radius: 6px;
  background: var(--panel-bg); color: var(--text-2); font-size: 11px;
}
</style>
