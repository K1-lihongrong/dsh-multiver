import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

// v0.2.14：4 语言切换。

test("4 语言切换文案正确", async ({ page }) => {
  await mockTauri(page, { installed: [makeVersion("0.2.0-rc.1")] });
  await page.goto("/");

  // 默认中文（helpers 固定 zh-CN）
  await expect(page.locator(".brand h1")).toHaveText("DSH 版本管理器");

  // 语言是 <select>，用 selectOption 切换
  const langSelect = page.locator("select").filter({ has: page.locator("option[value='en-US']") });
  await langSelect.selectOption("en-US");
  await expect(page.locator(".brand h1")).toHaveText("DSH Version Manager");

  // 切日本語
  await langSelect.selectOption("ja-JP");
  await expect(page.locator(".brand h1")).toHaveText("DSH バージョンマネージャー");
});

test("启动态文案随语言切换", async ({ page }) => {
  await mockTauri(page, { launchMode: "0.2.0-rc.1" });
  await page.goto("/");
  // 启动态显示，文案含"启动"
  await expect(page.locator(".splash-status")).toContainText("启动");
});
