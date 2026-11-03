import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, hover, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 03 已安装版本：对应正文 115-128 行。
// "每个版本显示：版本号、安装日期、是否默认、是否隔离、占用大小"
// "可用操作：运行 / 浏览器打开 / 设为默认 / 隔离 / 快捷方式 / 卸载 / 扫描占用 / 插件"
//
// 占用扫描走 scan_versions + scan-size-progress 事件，需手动推事件序列。
const SIZES = {
  "0.2.1-rc.2": { total: 1483237376, shared_size: 1181116006, exclusive_size: 302121370, shared_count: 12045 },
  "0.2.0-rc.1": { total: 1520410624, shared_size: 1181116006, exclusive_size: 339294618, shared_count: 12088 },
  "0.1.8": { total: 1398472704, shared_size: 1181116006, exclusive_size: 217356698, shared_count: 11902 },
};

/** 模拟一次完整的扫描事件流（两段式：counting → total → detail → 完成）。 */
async function playScan(page, versions) {
  const total = versions.length;
  for (let i = 0; i < total; i++) {
    const v = versions[i];
    const info = SIZES[v];
    const fileTotal = 12045;
    // 第一段：counting（不确定进度，done 是已发现文件数）
    for (const done of [1200, 4800, 9200, fileTotal]) {
      await page.evaluate((p) => window.__mockEmit("scan-size-progress", p), {
        version: v, stage: "counting", index: i + 1, total, done, fileTotal: 0, info: null, cached: false,
      });
      await page.waitForTimeout(120);
    }
    // 第一段完成：total（info 只含总量）
    await page.evaluate((p) => window.__mockEmit("scan-size-progress", p), {
      version: v, stage: "total", index: i + 1, total, done: 0, fileTotal,
      info: { total: info.total, shared_size: 0, exclusive_size: 0, shared_count: 0 }, cached: false,
    });
    await page.waitForTimeout(200);
    // 第二段：detail（文件级进度）
    for (const done of [3000, 7000, fileTotal]) {
      await page.evaluate((p) => window.__mockEmit("scan-size-progress", p), {
        version: v, stage: "detail", index: i + 1, total, done, fileTotal, info: null, cached: false,
      });
      await page.waitForTimeout(120);
    }
    // 该版本扫完：detail + 完整 info
    await page.evaluate((p) => window.__mockEmit("scan-size-progress", p), {
      version: v, stage: "detail", index: i + 1, total, done: fileTotal, fileTotal, info, cached: false,
    });
    await page.waitForTimeout(500);
  }
}

test("03 已安装版本与占用", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: "0.2.1-rc.2",
      isolated_versions: ["0.2.0-rc.1"],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [
      makeVersion("0.2.1-rc.2", { isDefault: true, installed_at: "2026-10-01" }),
      makeVersion("0.2.0-rc.1", { isolated: true, installed_at: "2026-09-28" }),
      makeVersion("0.1.8", { installed_at: "2026-09-20" }),
    ],
    extra: {
      // scan_versions 立即返回，实际进度靠下面手动推事件
      scan_versions: "() => Promise.resolve()",
      cancel_scan: "() => Promise.resolve()",
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 1) 点「扫描全部占用」，推完整事件流 → 显示"占用 X（复用 Y / 独占 Z）"
  const scanBtn = page.getByRole("button", { name: /扫描全部占用/ }).first();
  await clickAt(page, scanBtn);
  await page.waitForTimeout(300);
  await playScan(page, ["0.2.1-rc.2", "0.2.0-rc.1", "0.1.8"]);
  await page.waitForTimeout(600);

  // 2) 依次悬停各操作按钮，让观众看清可用的 8 个操作
  for (const name of ["运行", "浏览器打开", "设为默认", "快捷方式"]) {
    const btn = page.getByRole("button", { name, exact: true }).first();
    if (await btn.count()) await hover(page, btn);
  }

  // 3) 点「插件」按钮，展示 v0.3.4 的展开态（正文 128 行）
  const pluginBtn = page.getByRole("button", { name: "插件", exact: true }).first();
  if (await pluginBtn.count()) {
    await clickAt(page, pluginBtn);
    await page.waitForTimeout(700);
    await hover(page, pluginBtn);
    // 收起，保持截图干净
    await clickAt(page, pluginBtn);
    await page.waitForTimeout(400);
  }

  // 4) 展示「已隔离 ▾」菜单按钮（正文 124 行）
  const isoBtn = page.getByRole("button", { name: /已隔离/ }).first();
  if (await isoBtn.count()) await hover(page, isoBtn);

  await page.waitForTimeout(HOLD_END);
});
