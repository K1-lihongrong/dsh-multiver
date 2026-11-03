import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, scrollTo, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 12 路径设置：展示根目录输入框、目录列表，点「展开全部目录」展示额外目录。
test("12 路径设置", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      home_dir: "D:/dsh-data/home",
      store_dir: "D:/dsh-data/store",
      cache_dir: "D:/dsh-data/cache",
      state_dir: "D:/dsh-data/state",
      extra_dirs: [
        { name: "logs", desc: "启动错误与维护日志", path: "D:/dsh-data/logs" },
        { name: "webview", desc: "各版本内嵌窗口数据", path: "D:/dsh-data/webview" },
        { name: "trash", desc: "卸载回收站", path: "D:/dsh-data/trash" },
        { name: "assets", desc: "内部资源（快捷方式图标）", path: "D:/dsh-data/assets" },
      ],
      default_version: null,
      isolated_versions: [],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" })],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 滚到路径设置区
  await scrollTo(page, "#path-settings", -80);
  await page.waitForTimeout(800);

  // 点「展开全部目录」
  const expand = page.getByRole("button", { name: /展开全部目录/ }).first();
  if (await expand.count()) {
    await clickAt(page, expand);
    await page.waitForTimeout(800);
  }

  // 悬停目录行，展示"点击即在文件管理器打开"（正文 215 行）
  for (const idx of [1, 2]) {
    const item = page.locator(".path-item").nth(idx);
    if (await item.count()) {
      const ib = await item.boundingBox();
      if (ib) {
        await glide(page, ib.x + ib.width / 2, ib.y + ib.height / 2);
        await page.waitForTimeout(650);
      }
    }
  }

  // 正文 221 行：「导出配置」和「导入配置」按钮
  for (const name of [/导出配置/, /导入配置/]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(750);
      }
    }
  }
  await page.waitForTimeout(HOLD_END);
});
