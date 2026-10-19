import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

// v0.2.14：启动态（快捷方式启动 / 运行按钮）。

test("快捷方式启动：首帧即显示启动态（不闪主界面）", async ({ page }) => {
  await mockTauri(page, {
    launchMode: "0.2.0-rc.1",
    installed: [makeVersion("0.2.0-rc.1")],
  });
  await page.goto("/");
  // 启动态覆盖层可见
  await expect(page.locator(".launch-overlay")).toBeVisible();
  await expect(page.locator(".launch-version")).toHaveText("0.2.0-rc.1");
  await expect(page.locator(".launch-spinner")).toBeVisible();
});

test("快捷方式启动：静态骨架在 Vue 挂载后移除", async ({ page }) => {
  await mockTauri(page, { launchMode: "0.2.0-rc.1" });
  await page.goto("/");
  await expect(page.locator("#boot-skeleton")).toHaveCount(0);
});

test("普通启动：不显示启动态", async ({ page }) => {
  await mockTauri(page, { launchMode: null, installed: [makeVersion("0.2.0-rc.1")] });
  await page.goto("/");
  await expect(page.locator(".launch-overlay")).toHaveCount(0);
  await expect(page.locator(".brand h1")).toBeVisible();
});

test("运行按钮：点击后切启动态", async ({ page }) => {
  await mockTauri(page, {
    launchMode: null,
    // 用隔离版本：跳过"共享 home"确认框（否则启动态会被对话框挡住）
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      // run_version 延迟返回（字符串函数体，因 addInitScript 参数不能传函数）
      run_version: "(args) => new Promise((r) => setTimeout(() => r('已在窗口打开 DSH 0.2.0-rc.1'), 500))",
    },
  });
  await page.goto("/");
  // 点「运行」
  await page.getByRole("button", { name: "运行", exact: true }).click();
  // 启动态出现
  await expect(page.locator(".launch-overlay")).toBeVisible();
  // 500ms 后消失（恢复主界面）
  await expect(page.locator(".launch-overlay")).toHaveCount(0, { timeout: 3000 });
});
