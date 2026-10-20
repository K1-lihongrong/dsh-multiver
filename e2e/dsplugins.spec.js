import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

// v0.3.0 双向桥 B1：dsh 插件管理面板。

test("点「插件」→ 展开面板（列 profile 与插件）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: [
        {
          name: "web",
          path: "D:/dsh/profiles/web",
          plugins: [
            { name: "@deepseek-ai/dsh-base", version: "0.2.1", is_bundle: true },
            { name: "my-plugin", version: "1.0.0", is_bundle: false },
          ],
        },
      ],
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await expect(page.locator(".ds-plugin-panel")).toBeVisible();
  await expect(page.getByText("@deepseek-ai/dsh-base")).toBeVisible();
  await expect(page.getByText("my-plugin")).toBeVisible();
});

test("无 profile 时显示提示", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: { ds_list_plugins: [] },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await expect(page.locator(".dp-empty")).toContainText("profile");
});

test("输入包名点安装 → 调 ds_plugin_action", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordCall", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: [{ name: "web", path: "D:/p", plugins: [] }],
      ds_plugin_action: "(args) => { window.__recordCall(args); return Promise.resolve('已安装 ' + args.package); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByPlaceholder(/npm 包名/).fill("test-pkg");
  // 「安装」在页面里有两个（可用版本列表 + 插件面板），限定在面板内
  await page.locator(".ds-plugin-panel").getByRole("button", { name: "安装", exact: true }).click();
  await page.waitForTimeout(400);
  const flat = JSON.stringify(calls);
  expect(flat).toContain("test-pkg");
  expect(flat).toContain('"action":"add"');
  expect(flat).toContain('"profile":"web"');
});
