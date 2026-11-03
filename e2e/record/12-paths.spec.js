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

  // 悬停某一个目录行，展示可点击的视觉提示
  const item = page.locator(".path-item").nth(2);
  if (await item.count()) {
    const ib = await item.boundingBox();
    if (ib) {
      await glide(page, ib.x + ib.width / 2, ib.y + ib.height / 2);
      await page.waitForTimeout(700);
    }
  }
  await page.waitForTimeout(HOLD_END);
});
