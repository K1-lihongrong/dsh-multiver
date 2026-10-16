// v0.2.13 占用扫描状态管理（统一入口：runScan）。
//
// 命令：scan_versions(versions[]) + cancel_scan()
// 事件 "scan-size-progress" payload:
//   { version, stage:"counting"|"total"|"detail", index, total, done, fileTotal, info|null, cached }
//   · counting：第一段遍历中（不确定进度），done=已发现文件数
//   · total：第一段完成，info 为仅含总量的结果，fileTotal=文件数
//   · detail 且 info=null：第二段文件级进度（done/fileTotal）
//   · detail 且 info≠null：该版本扫完；cached=true 表示命中缓存复用
//
// 动效时序以 docs/v0.2.13-动效规范.md 为准。

import { ref, nextTick, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/// 闪烁动画时长（与 CSS scan-flash 对齐）
const FLASH_MS = 1200;
/// 复用标签最小完整展示时间
const CACHED_MIN_VISIBLE_MS = 1500;
/// 复用标签渐隐时长（与 CSS badge-cached transition 对齐）
const CACHED_FADE_MS = 1200;
/// 渐隐后清空的缓冲
const FADE_BUFFER_MS = 100;

/// 刚扫完需要闪烁的版本号集合（FLASH_MS 后自动清除）
export const flashVersions = ref(new Set());

function flash(version) {
  // 先移除（若已在集合），nextTick 后加回，强制 class 变化 → CSS 动画重播
  const s = new Set(flashVersions.value);
  s.delete(version);
  flashVersions.value = s;
  nextTick(() => {
    const s2 = new Set(flashVersions.value);
    s2.add(version);
    flashVersions.value = s2;
    setTimeout(() => {
      const t = new Set(flashVersions.value);
      t.delete(version);
      flashVersions.value = t;
    }, FLASH_MS);
  });
}

export function useSizeScan() {
  const verSizes = ref({});      // 版本号 -> SizeInfo
  const scanning = ref(false);   // 是否有扫描进行中
  const queue = ref([]);         // 本次待扫版本列表
  const current = ref(null);     // 正在扫的版本号
  const progress = ref(null);    // { version, index, total, done, fileTotal, stage }
  const single = ref(false);     // 是否单版本扫描（顶部不出总进度条）
  const cachedSet = ref(new Set()); // 本次复用缓存的版本号
  const fadingOut = ref(false);  // cachedSet 是否正在渐隐

  let unlisten = null;
  let fadeTimer = null;
  let fadeClearTimer = null;
  let fadeScheduleTimer = null;
  let cachedFirstShownAt = 0;    // 标签首次出现时间（用于最小展示）

  async function ensureListener() {
    if (unlisten) return;
    unlisten = await listen("scan-size-progress", (e) => {
      const p = e.payload;
      if (!p || !p.version) return;
      // 该版本扫完（带完整 info）：即使扫描刚结束也要处理（可能被 finally 赶在前面到达），
      // 否则会丢掉最后一次闪烁。故完成事件不受会话守卫限制。
      if (p.info) {
        verSizes.value = { ...verSizes.value, [p.version]: p.info };
        // 扫完即从待扫队列移除：避免该行仍带 item-queued（浅灰背景）覆盖闪烁
        if (queue.value.includes(p.version)) {
          queue.value = queue.value.filter((x) => x !== p.version);
        }
        // 复用缓存的版本：记入 cachedSet（显示"已复用缓存"标签）
        if (p.cached) {
          // 每个新标签都重置"首次展示"计时：保证新加入的标签也满足最小展示
          cachedFirstShownAt = Date.now();
          const s = new Set(cachedSet.value);
          s.add(p.version);
          cachedSet.value = s;
          fadingOut.value = false;
        }
        // 扫描完成即闪烁（命中缓存也闪）：表示"这一行处理完了"
        if (p.stage === "detail") {
          flash(p.version);
        }
        progress.value = { version: p.version, index: p.index, total: p.total, done: 0, fileTotal: p.fileTotal || 0, stage: p.stage };
        return;
      }
      // 进度事件：扫描已结束则丢弃（避免回写 current/progress，造成"扫描已停但界面又显示扫描中"）
      if (!scanning.value) return;
      current.value = p.version;
      progress.value = { version: p.version, index: p.index, total: p.total, done: p.done, fileTotal: p.fileTotal || 0, stage: p.stage };
    });
  }

  /// 启动"复用标签渐隐"（确保最小展示时间）。
  function scheduleFade() {
    if (fadeTimer) { clearTimeout(fadeTimer); fadeTimer = null; }
    if (fadeClearTimer) { clearTimeout(fadeClearTimer); fadeClearTimer = null; }
    if (cachedSet.value.size === 0) return;
    const shown = Date.now() - cachedFirstShownAt;
    const wait = Math.max(0, CACHED_MIN_VISIBLE_MS - shown);
    fadeTimer = setTimeout(() => {
      fadeTimer = null;
      fadingOut.value = true;
      fadeClearTimer = setTimeout(() => {
        cachedSet.value = new Set();
        fadingOut.value = false;
        cachedFirstShownAt = 0;
        fadeClearTimer = null;
      }, CACHED_FADE_MS + FADE_BUFFER_MS);
    }, wait);
  }

  /// 统一入口。versions 传数组。
  async function runScan(versions) {
    if (scanning.value) return;
    // 点击立即反馈：先设状态，不等后端
    scanning.value = true;
    queue.value = [...versions];
    single.value = versions.length === 1;
    current.value = versions[0] || null;
    progress.value = { version: versions[0] || null, index: 1, total: versions.length, done: 0, fileTotal: 0, stage: "counting" };
    // 连续点击：保留旧标签，只中断正在进行的渐隐（含未触发的调度计时器）
    if (fadeTimer) { clearTimeout(fadeTimer); fadeTimer = null; }
    if (fadeClearTimer) { clearTimeout(fadeClearTimer); fadeClearTimer = null; }
    if (fadeScheduleTimer) { clearTimeout(fadeScheduleTimer); fadeScheduleTimer = null; }
    fadingOut.value = false;
    try {
      await ensureListener();
      await invoke("scan_versions", { versions });
    } finally {
      scanning.value = false;
      queue.value = [];
      current.value = null;
      progress.value = null;
      single.value = false;
      // 等最后的事件被处理完（一个微任务周期），再安排渐隐
      fadeScheduleTimer = setTimeout(scheduleFade, 0);
    }
  }

  /// 取消：立即请求中断。
  async function cancelScan() {
    if (!scanning.value) return;
    try { await invoke("cancel_scan"); } catch (_) {}
  }

  onUnmounted(() => {
    if (unlisten) { unlisten(); unlisten = null; }
    if (fadeTimer) { clearTimeout(fadeTimer); fadeTimer = null; }
    if (fadeClearTimer) { clearTimeout(fadeClearTimer); fadeClearTimer = null; }
    if (fadeScheduleTimer) { clearTimeout(fadeScheduleTimer); fadeScheduleTimer = null; }
  });

  return {
    verSizes, scanning, queue, current, progress, single,
    cachedSet, fadingOut, runScan, cancelScan, flashVersions,
  };
}
