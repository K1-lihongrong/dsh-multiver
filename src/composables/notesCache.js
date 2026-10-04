import { ref } from "vue";

// 更新说明的**模块级**缓存——跨 NotesPanel 实例共享。
// 为什么需要：NotesPanel 每次挂载（点新版本、或换到"可用版本/已安装版本"）都是新实例，
// 若缓存放在组件 <script setup> 里，每个实例各一份，导致同一版本反复请求 GitHub。
// 放在这里后，任何实例加载过某版本，其它实例直接命中。
//
// 结构：version -> { status: 'loading'|'ok'|'error', html, url, empty }
export const notesCache = ref({});

/// 是否已有该版本的缓存（含"加载中"，用于并发去重）
export function hasNotes(v) {
  return !!notesCache.value[v];
}

/// 写入某版本的缓存条目
export function setNotes(v, entry) {
  notesCache.value = { ...notesCache.value, [v]: entry };
}
