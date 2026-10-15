// v0.2.13 占用扫描状态管理（两段式：先秒出总量，再后台补精确 + 进度）。
//
// 与后端约定（事件 "scan-size-progress"）：
//   payload: { version, stage:"total"|"detail", index, total, done, fileTotal, info|null }
//   - stage="total"：第一段，带 info（仅总量，shared=0），fileTotal=文件数
//   - stage="detail" 且 info=null：文件级进度，done/fileTotal
//   - stage="detail" 且 info≠null：该版本扫完，info 为完整结果
//
// 命令：
//   invoke("scan_all_sizes")                扫描全部（两段式 + 进度事件）
//   invoke("scan_version_size_streaming", {version})  单版本（两段式 + 进度事件）
//   invoke("scan_version_size", {version})  单版本（一次性返回，保留兼容）

import { ref, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/// 刚扫完、需要闪烁提示的版本号集合（0.6s 后自动清除）
export const flashVersions = ref(new Set());

function flash(version) {
  const s = new Set(flashVersions.value);
  s.add(version);
  flashVersions.value = s;
  setTimeout(() => {
    const t = new Set(flashVersions.value);
    t.delete(version);
    flashVersions.value = t;
  }, 600);
}

export function useSizeScan() {
  const verSizes = ref({});   // 版本号 -> SizeInfo
  const scanning = ref(false); // 是否正在"扫描全部"
  const scanningVer = ref(null); // 正在扫描的单版本
  const progress = ref(null);  // { version, index, total, done, fileTotal }
  const reused = ref(false);   // 是否刚发生"全部命中缓存"

  let unlisten = null;
  let reusedTimer = null;

  async function ensureListener() {
    if (unlisten) return;
    unlisten = await listen("scan-size-progress", (e) => {
      const p = e.payload;
      if (!p) return;
      // 全部命中缓存：显示"已复用上次扫描"提示（2s 后渐隐）
      if (p.reused) {
        reused.value = true;
        if (reusedTimer) clearTimeout(reusedTimer);
        reusedTimer = setTimeout(() => { reused.value = false; }, 2000);
        return;
      }
      // 该版本扫完（带完整 info）：写结果 + 闪烁
      if (p.info) {
        verSizes.value = { ...verSizes.value, [p.version]: p.info };
        progress.value = { version: p.version, index: p.index, total: p.total, done: 0, fileTotal: p.fileTotal || 0 };
        // 第一段（total）不闪烁，只更新结果；第二段完成才闪
        if (p.stage === "detail") flash(p.version);
        return;
      }
      // 文件级进度（detail 无 info）
      if (p.stage === "detail") {
        progress.value = { version: p.version, index: p.index, total: p.total, done: p.done, fileTotal: p.fileTotal || 0 };
      }
    });
  }

  async function scanAll() {
    if (scanning.value) return;
    scanning.value = true;
    progress.value = null;
    try {
      await ensureListener();
      await invoke("scan_all_sizes");
    } finally {
      scanning.value = false;
      progress.value = null;
    }
  }

  async function scanOne(version) {
    scanningVer.value = version;
    progress.value = null;
    try {
      await ensureListener();
      await invoke("scan_version_size_streaming", { version });
    } finally {
      scanningVer.value = null;
      progress.value = null;
    }
  }

  onUnmounted(() => {
    if (unlisten) { unlisten(); unlisten = null; }
  });

  return { verSizes, scanning, scanningVer, progress, reused, scanAll, scanOne, flashVersions };
}
