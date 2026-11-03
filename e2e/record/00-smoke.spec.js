import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 冒烟录屏：只验证录屏管线能跑通（不产出正式手册图）。
// 跑法：pnpm exec playwright test --config=playwright.record.config.js
test("录屏管线冒烟", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    installed: [makeVersion("0.2.1-rc.2", { isDefault: true })],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);
  // 移动鼠标到「运行」按钮并悬停
  const btn = page.getByRole("button", { name: "运行", exact: true }).first();
  await clickAt(page, btn);
  await page.waitForTimeout(1200);
  await page.waitForTimeout(HOLD_END);
});
