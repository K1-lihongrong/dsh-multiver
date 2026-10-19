import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

// v0.2.14：共享 home 确认对话框（含"我知道了"）。

test("非隔离版本点运行 → 弹共享 home 确认框", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: false })],
  });
  await page.goto("/");
  await page.getByRole("button", { name: "运行", exact: true }).click();
  // 对话框出现
  await expect(page.locator(".modal-mask")).toBeVisible();
  await expect(page.getByText("共享 home 提示")).toBeVisible();
  // 三个按钮
  await expect(page.getByRole("button", { name: "取消" })).toBeVisible();
  await expect(page.getByRole("button", { name: "我知道了（以后不再提示）" })).toBeVisible();
  await expect(page.getByRole("button", { name: "继续运行" })).toBeVisible();
});

test("点「取消」→ 对话框关闭、不启动", async ({ page }) => {
  await mockTauri(page, { installed: [makeVersion("0.2.0-rc.1")] });
  await page.goto("/");
  await page.getByRole("button", { name: "运行", exact: true }).click();
  await page.getByRole("button", { name: "取消" }).click();
  await expect(page.locator(".modal-mask")).toHaveCount(0);
  // 没进启动态
  await expect(page.locator(".launch-overlay")).toHaveCount(0);
});

test("点「我知道了」→ 对话框关闭 + 不再提示", async ({ page }) => {
  await mockTauri(page, { installed: [makeVersion("0.2.0-rc.1")] });
  await page.goto("/");
  await page.getByRole("button", { name: "运行", exact: true }).click();
  await page.getByRole("button", { name: "我知道了（以后不再提示）" }).click();
  await expect(page.locator(".modal-mask")).toHaveCount(0);
  // localStorage 存了"不再提示"
  const v = await page.evaluate(() => localStorage.getItem("dsh-multiver.warnSharedHome"));
  expect(v).toBe("0");
});
