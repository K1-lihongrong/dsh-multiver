<script setup>
import { ref, computed, inject } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";
import { actionErrorText } from "../composables/useDsPluginErrors.js";
import { DS_PLUGIN_LOG_KEY } from "../composables/useDsPluginLog.js";

// 「已安装」标签页：筛选、卸载、检查更新、更新。
// 从 DsPluginPanel 拆出（v0.3.3 D21）。官方语义标签的判定也在这里。
const props = defineProps({
  version: { type: String, required: true },
  profile: { type: String, default: "" },
  plugins: { type: Array, default: () => [] },
  busy: { type: Boolean, default: false },
});
const emit = defineEmits(["update:busy", "reload", "refresh-info", "action-done", "action-failed"]);

const { notify } = useToast();
const { logLines, showLog } = inject(DS_PLUGIN_LOG_KEY);

// v0.3.0 B1：与 dsh 官方插件页语义对齐。
// 「随 DSH 提供」= 官方安装随附的 bundle（OPTIONAL/DEFAULT/ON_DEMAND，后端从 dsh-app-boot 读取）；
// 「已安装」= profile dependencies；「插件 / 依赖」= 包是否声明 dsh.bundle（官方定义）。
const isOfficial = (pl) => pl.official === true;
const srcLabel = (pl) => t(isOfficial(pl) ? "dsplugins.sourceBundle" : "dsplugins.sourceUser");
const kindLabel = (pl) => t(pl.declares_bundle ? "dsplugins.kindPlugin" : "dsplugins.kindDependency");

// B1-10：按包名/版本过滤
const search = ref("");
const filteredPlugins = computed(() => {
  const q = search.value.trim().toLowerCase();
  if (!q) return props.plugins;
  return props.plugins.filter(
    (pl) =>
      pl.name.toLowerCase().includes(q) ||
      String(pl.version || "").toLowerCase().includes(q),
  );
});

// 可更新插件（包名 → 最新版本信息）
const updates = ref({});

// B1-13：检查可更新插件（dsh CLI `outdated --json`）
async function checkUpdates() {
  if (!props.profile) return;
  emit("update:busy", true);
  try {
    const list = await invoke("ds_check_updates", {
      version: props.version,
      profile: props.profile,
    });
    const map = {};
    for (const u of list || []) map[u.name] = u;
    updates.value = map;
    notify(t("dsplugins.updatesFound", { n: Object.keys(map).length }));
  } catch (e) {
    notify("" + e);
  } finally {
    emit("update:busy", false);
  }
}

// B1-13：更新 = 官方语义「先卸载再安装新版」
async function updatePlugin(pl) {
  const u = updates.value[pl.name];
  if (!u) return;
  const ok = await ask(t("dsplugins.updateConfirm", { p: pl.name, v: u.latest }), {
    title: t("dsplugins.update"),
    kind: "warning",
  });
  if (!ok) return;
  emit("update:busy", true);
  logLines.value = [];
  try {
    await invoke("ds_plugin_action", {
      version: props.version,
      profile: props.profile,
      action: "remove",
      package: pl.name,
    });
    await invoke("ds_plugin_action", {
      version: props.version,
      profile: props.profile,
      action: "add",
      package: `${pl.name}@${u.latest}`,
    });
    notify(t("dsplugins.updated", { p: pl.name, v: u.latest }));
    emit("reload");
    await checkUpdates();
  } catch (e) {
    notify(actionErrorText(e));
    showLog.value = true;
  } finally {
    emit("update:busy", false);
  }
}

/// 卸载单个包。真正的执行走 runAction，供面板的「允许构建脚本后重试」复用。
function removePlugin(pkg, locked) {
  return runAction("remove", pkg, locked);
}

/// 执行一次插件操作（add / remove）。
///
/// 面板底部「构建脚本已允许，重试上次操作」也调用本方法，故失败时把
/// 上次操作回传给壳保存。
async function runAction(action, pkg, locked) {
  const name = (pkg || "").trim();
  if (!name) return;
  // 仅 DSH 随附的官方组合包不允许单独卸载；用户安装的 bundle 可卸载。
  if (action === "remove" && locked) {
    notify(t("dsplugins.bundleLocked"));
    return;
  }
  // 卸载是破坏性操作：先二次确认（与项目其他卸载操作一致）。
  if (action === "remove") {
    const ok = await ask(t("dsplugins.removeConfirm", { p: name }), {
      title: t("dsplugins.remove"),
      kind: "warning",
    });
    if (!ok) return;
  }
  emit("update:busy", true);
  logLines.value = [];
  try {
    await invoke("ds_plugin_action", {
      version: props.version,
      profile: props.profile,
      action,
      package: name,
    });
    notify(t(action === "add" ? "dsplugins.installed" : "dsplugins.removed", { p: name }));
    emit("action-done");
    if (logLines.value.length) showLog.value = true;
    emit("reload");
  } catch (e) {
    emit("action-failed", { action, name, bundle: !!locked });
    notify(actionErrorText(e));
    showLog.value = true;
    emit("refresh-info");
  } finally {
    emit("update:busy", false);
  }
}

// 工具栏的「检查更新」按钮在壳里，通过 ref 调用本方法；
// 「允许构建脚本后重试」也由壳通过 ref 调用 runAction。
defineExpose({ checkUpdates, runAction });
</script>

<template>
  <div v-if="!plugins.length" class="dp-state dp-state-empty">
    <span class="dp-state-icon">🧩</span>
    <span>{{ t("dsplugins.empty") }}</span>
  </div>
  <template v-else>
    <!-- 列表筛选：只过滤已安装项，与上方「添加插件」职责分离 -->
    <div class="dp-filter">
      <span class="dp-filter-icon" aria-hidden="true">🔍</span>
      <input
        v-model="search"
        class="dp-input dp-search"
        :placeholder="t('dsplugins.searchPlaceholder')"
        :aria-label="t('dsplugins.searchPlaceholder')"
      />
    </div>
    <ul v-if="filteredPlugins.length" class="dp-list">
      <li v-for="pl in filteredPlugins" :key="pl.name" class="dp-item">
        <div class="dp-item-main">
          <span class="dp-item-icon">{{ pl.is_bundle ? "📦" : "🧩" }}</span>
          <div class="dp-item-info">
            <code class="dp-item-name" :title="pl.description || ''">{{ pl.name }}</code>
            <span
              class="dp-item-ver"
              v-if="pl.version"
              :title="pl.spec ? t('dsplugins.specTip', { spec: pl.spec }) : ''"
            >v{{ pl.version }}</span>
          </div>
          <span class="dp-badge" :class="isOfficial(pl) ? 'dp-badge-bundle' : 'dp-badge-user'">{{ srcLabel(pl) }}</span>
          <span class="dp-badge dp-badge-kind" v-if="!pl.declares_bundle">{{ kindLabel(pl) }}</span>
          <span class="dp-badge dp-badge-update" v-if="updates[pl.name]">
            {{ t("dsplugins.newVersion", { v: updates[pl.name].latest }) }}
          </span>
        </div>
        <button
          v-if="updates[pl.name]"
          class="dp-btn dp-btn-primary dp-btn-sm"
          @click="updatePlugin(pl)"
          :disabled="busy"
        >
          {{ t("dsplugins.update") }}
        </button>
        <button
          class="dp-btn dp-btn-danger dp-btn-sm"
          @click="removePlugin(pl.name, isOfficial(pl))"
          :disabled="busy || isOfficial(pl)"
          :title="isOfficial(pl) ? t('dsplugins.bundleLocked') : ''"
        >
          {{ t("dsplugins.remove") }}
        </button>
      </li>
    </ul>
    <div v-else class="dp-state dp-state-empty">{{ t("dsplugins.noMatch") }}</div>
  </template>
</template>
