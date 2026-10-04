<script setup>
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

// 环境自检面板：自己调 check_env、自己管状态。
// 父组件通过 ref 调用 run()（安装前检查）；错误经 open-url 事件交给父组件打开链接。
const emit = defineEmits(["open-url", "error"]);

const checks = ref([]);
const checked = ref(false);
const passed = ref(false);
const loading = ref(false);

/// 运行环境检查，返回是否通过（致命项全 ok）。
async function run() {
  loading.value = true;
  try {
    checks.value = await invoke("check_env");
    checked.value = true;
    passed.value = checks.value.filter((c) => c.critical).every((c) => c.ok);
    return passed.value;
  } catch (e) {
    emit("error", "环境检查失败: " + e);
    return false;
  } finally {
    loading.value = false;
  }
}

onMounted(run);

defineExpose({ run });
</script>

<template>
  <section class="panel">
    <div class="panel-head">
      <h2>
        环境检查
        <span class="badge-ok" v-if="checked && passed">通过</span>
        <span class="badge-err" v-else-if="checked">未通过</span>
      </h2>
      <button class="btn small" @click="run" :disabled="loading">重新检查</button>
    </div>
    <ul class="env-list" v-if="checks.length">
      <li
        v-for="c in checks"
        :key="c.name"
        class="env-item"
        :class="{ 'env-item-fail': !c.ok }"
      >
        <div class="env-row">
          <span class="env-icon" :class="c.ok ? 'ok' : 'err'">{{ c.ok ? "✓" : "✕" }}</span>
          <span class="env-name">{{ c.name }}</span>
          <span class="env-detail">{{ c.detail }}</span>
          <span class="env-critical" v-if="!c.ok && !c.critical">（非致命）</span>
        </div>
        <div class="env-guide" v-if="!c.ok && c.install_hint">
          <span class="env-guide-text">{{ c.install_hint }}</span>
          <button
            v-if="c.install_url"
            class="btn small primary"
            @click="emit('open-url', c.install_url)"
          >打开下载页</button>
        </div>
      </li>
    </ul>
    <div v-else class="hint">正在检查环境...</div>
  </section>
</template>

<style scoped>
.env-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.env-item {
  display: flex; flex-direction: column; align-items: stretch; gap: 0;
  padding: 8px 12px; border-radius: 8px; background: #fafbfc;
  font-size: 13px;
}
.env-icon {
  width: 18px; height: 18px; border-radius: 50%; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center;
  font-size: 11px; font-weight: 700; color: #fff;
}
.env-icon.ok { background: #30a46c; }
.env-icon.err { background: #e5484d; }
.env-name { font-weight: 600; color: #374151; min-width: 90px; }
.env-detail { color: #6b7280; font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.env-critical { color: #e5484d; font-size: 11px; }
.env-item-fail { background: #fff8f8; }
.env-row { display: flex; align-items: center; gap: 10px; }
.env-guide {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  margin-top: 8px; margin-left: 28px;
  padding: 8px 10px; background: #fff; border: 1px solid #f0d0d0;
  border-radius: 6px; font-size: 12px; color: #6b7280; line-height: 1.5;
}
.env-guide-text { flex: 1; min-width: 200px; }
.badge-ok { background: #e8f5ec; color: #2f9e5f; font-size: 11px; padding: 2px 8px; border-radius: 10px; margin-left: 6px; }
.badge-err { background: #fdf2f2; color: #d9534f; font-size: 11px; padding: 2px 8px; border-radius: 10px; margin-left: 6px; }
</style>
