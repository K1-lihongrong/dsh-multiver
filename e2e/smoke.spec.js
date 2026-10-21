import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

test("页面能正常渲染（不白屏）", async ({ page }) => {
  await mockTauri(page);
  await page.goto("/");
  // 主标题存在
  await expect(page.locator(".brand h1")).toBeVisible();
  // 有"环境检查"卡片
  await expect(page.getByText("环境检查").first()).toBeVisible();
  // 新增 git 检查项正常渲染
  await expect(page.getByText("git", { exact: true })).toBeVisible();
});

test("已安装版本列表渲染", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1"), makeVersion("0.2.1-alpha.1", { isolated: true })],
  });
  await page.goto("/");
  await expect(page.getByText("0.2.0-rc.1")).toBeVisible();
  await expect(page.getByText("0.2.1-alpha.1")).toBeVisible();
});
