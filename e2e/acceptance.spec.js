import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

test("浏览器打开返回失败结构时展示 URL（前端兜底能力）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.1-rc.2", { isolated: true })],
    extra: {
      open_in_browser: {
        url: "http://127.0.0.1:3080/?token=test-token",
        opened: false,
        message: "dsh 已启动，但无法自动打开浏览器（xdg-open 退出码 3）。请手动访问：http://127.0.0.1:3080/?token=test-token",
      },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "浏览器打开", exact: true }).click();
  await expect(page.locator(".browser-modal")).toContainText("无法自动打开浏览器");
  await expect(page.locator(".browser-url")).toHaveValue(
    "http://127.0.0.1:3080/?token=test-token",
  );
  await page.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(page.locator(".browser-modal")).toHaveCount(0);
});

test("L-29 更新检查失败显示错误而不是静默无更新", async ({ page }) => {
  await mockTauri(page, {
    extra: {
      check_update: { has_update: false, error: "GitHub API 返回 HTTP 403" },
    },
  });
  await page.goto("/");
  await expect(page.locator(".update-banner-error")).toContainText("检查更新失败");
  await expect(page.locator(".update-banner-error")).toContainText("HTTP 403");
  await page.locator(".update-banner-error").getByRole("button", { name: "×", exact: true }).click();
  await expect(page.locator(".update-banner-error")).toHaveCount(0);
});

test("L-31 镜像子包缺失时提供换源重试", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordInstall", (args) => calls.push(args));
  await mockTauri(page, {
    extra: {
      list_remote: [],
      install_version:
        "(args) => { window.__recordInstall(args); return Promise.reject('ERR_PNPM_NO_MATCHING_VERSION No matching version found for @aws-sdk/types@^3.975.0'); }",
    },
  });
  await page.goto("/");
  const input = page.locator(".install-row input");
  await input.fill("0.2.1-rc.2");
  await page.locator(".install-row button").click();
  await page.waitForTimeout(400);

  await expect(page.locator(".modal")).toContainText("镜像未同步");
  await expect(page.locator(".modal select")).toBeVisible();
  await page.locator(".modal select").selectOption("https://registry.npmjs.org/");
  await page.getByRole("button", { name: "换源重试", exact: true }).click();
  await page.waitForTimeout(400);

  expect(calls.length).toBeGreaterThanOrEqual(2);
  const flat = JSON.stringify(calls);
  expect(flat).toContain("registry.npmjs.org");
});
