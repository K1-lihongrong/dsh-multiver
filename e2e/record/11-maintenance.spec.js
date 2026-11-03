import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, scrollTo, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 11 维护面板：滚动到路径设置里的维护区，展示开关、上次执行时间与三个按钮。
test("11 维护面板", async ({ page }) => {
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
      maintenance: {
        auto_enabled: true,
        last_cleanup_at: "2026-11-02T08:15:00Z",
        last_cleanup_count: 3,
        last_prune_at: "2026-10-28T21:40:00Z",
      },
    },
    installed: [makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" })],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 滚到维护区（面板底部）
  await scrollTo(page, ".maint", -120);
  await page.waitForTimeout(700);

  // 悬停「清理缓存」按钮展示 hover 态
  const btn = page.getByRole("button", { name: /清理缓存|清理孤立缓存/ }).first();
  if (await btn.count()) {
    const bb = await btn.boundingBox();
    if (bb) {
      await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
      await page.waitForTimeout(700);
    }
  }
  await page.waitForTimeout(HOLD_END);
});
