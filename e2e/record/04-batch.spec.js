import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 04 批量操作栏：勾选两个版本 → 底部出现批量操作栏 → 悬停「批量卸载」展示危险按钮红底。
test("04 批量操作栏", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: null,
      isolated_versions: [],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [
      makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" }),
      makeVersion("0.2.0-rc.1", { installed_at: "2026-09-28" }),
      makeVersion("0.1.8", { installed_at: "2026-09-20" }),
    ],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);
  // 依次勾选前两个版本的复选框
  const boxes = page.locator(".ver-item .sel-box");
  await clickAt(page, boxes.nth(0));
  await page.waitForTimeout(300);
  await clickAt(page, boxes.nth(1));
  await page.waitForTimeout(500);
  // 悬停「批量卸载」展示危险按钮 hover 态
  const danger = page.getByRole("button", { name: /批量卸载/ }).first();
  if (await danger.count()) {
    const box = await danger.boundingBox();
    if (box) {
      await glide(page, box.x + box.width / 2, box.y + box.height / 2);
      await page.waitForTimeout(700);
    }
  }
  await page.waitForTimeout(HOLD_END);
});
