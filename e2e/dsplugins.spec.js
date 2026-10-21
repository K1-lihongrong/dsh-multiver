import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

// v0.3.0 双向桥 B1：dsh 插件管理面板。

const PROFILE = {
  name: "web",
  path: "D:/dsh/profiles/web",
  plugins: [
    { name: "@deepseek-ai/dsh-base", version: "0.2.1", is_bundle: true, source: "bundle", origin: "official" },
    { name: "my-plugin", version: "1.0.0", is_bundle: false, source: "user", origin: "third_party" },
  ],
};

// 官方 CLI 失败诊断 → 面板补充本地化提示
test("安装失败且带官方诊断 → 显示 allow-version 指引（对齐官方）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_action:
        "(args) => Promise.reject('dsh: installation rejected: incompatible\\ndsh: to accept the risk, run: dsh plugin --profile web allow-version x@1.0.0 --dsh-version 0.2.1 --accept-risk\\ndsh: plugin command failed; diagnostics: D:/dsh/profiles/web/.plugin-manager/logs/op/pnpm.log')",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByPlaceholder(/npm 包名/).fill("x");
  await page.locator(".ds-plugin-panel").getByRole("button", { name: "安装", exact: true }).click();
  await page.waitForTimeout(400);
  await expect(page.locator(".toast")).toContainText("版本不兼容");
  await expect(page.locator(".toast")).toContainText("诊断日志");
});


test("点「插件」→ 展开面板（列 profile 与插件 + 官方语义标签）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await expect(page.locator(".ds-plugin-panel")).toBeVisible();
  await expect(page.getByText("@deepseek-ai/dsh-base")).toBeVisible();
  await expect(page.getByText("my-plugin")).toBeVisible();
  // 与官方语义对齐：只标「随 DSH 提供 / 已安装」，不再按 npm scope 猜官方/第三方
  await expect(page.locator(".dp-badge-bundle")).toHaveText("随 DSH 提供");
  await expect(page.locator(".dp-badge-user")).toHaveText("已安装");
  await expect(page.locator(".dp-badge-official")).toHaveCount(0);
});

test("无 profile 时显示提示", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: { ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [] } },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await expect(page.locator(".dp-state-empty")).toBeVisible();
});

test("输入包名点安装 → 调 ds_plugin_action", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordCall", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [{ name: "web", path: "D:/p", plugins: [] }] },
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

test("内置 bundle 的卸载按钮禁用", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: { ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] } },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  const bundleRow = page.locator(".dp-item", { hasText: "@deepseek-ai/dsh-base" });
  await expect(bundleRow.getByRole("button", { name: "卸载" })).toBeDisabled();
});

test("卸载用户插件 → 二次确认后调 ds_plugin_action(remove)", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordRemove", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_action: "(args) => { window.__recordRemove(args); return Promise.resolve('ok'); }",
      // Tauri 对话框插件：ask() 走 plugin:dialog|message，返回 "Yes" 视为确认
      "plugin:dialog|message": "Yes",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  const row = page.locator(".dp-item", { hasText: "my-plugin" });
  await row.getByRole("button", { name: "卸载" }).click();
  await page.waitForTimeout(500);
  const flat = JSON.stringify(calls);
  expect(flat).toContain('"action":"remove"');
  expect(flat).toContain("my-plugin");
});

test("无 profile 时可初始化 profile（B1-07）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordInit", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [] },
      ds_init_profile: "(args) => { window.__recordInit(args); return Promise.resolve('ok'); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByPlaceholder(/profile 名/).fill("web");
  await page.getByRole("button", { name: "初始化 profile", exact: true }).click();
  await page.waitForTimeout(500);
  const flat = JSON.stringify(calls);
  expect(flat).toContain('"profile":"web"');
  expect(flat).toContain('"version":"0.2.0-rc.1"');
});

test("搜索过滤插件（B1-10）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: { ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] } },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByPlaceholder("搜索插件…").fill("my-plugin");
  await expect(page.locator(".dp-item", { hasText: "my-plugin" })).toBeVisible();
  await expect(page.locator(".dp-item", { hasText: "@deepseek-ai/dsh-base" })).toHaveCount(0);
});

test("安装失败 → 显示错误提示（B1-09）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_action: "(args) => Promise.reject('模拟安装失败：网络错误')",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByPlaceholder(/npm 包名/).fill("bad-pkg");
  await page.locator(".ds-plugin-panel").getByRole("button", { name: "安装", exact: true }).click();
  await page.waitForTimeout(400);
  await expect(page.locator(".toast")).toContainText("模拟安装失败");
});

test("打开 profile 目录 → 调 ds_open_profile_dir（B1-09）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordOpenDir", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_open_profile_dir: "(args) => { window.__recordOpenDir(args); return Promise.resolve(); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.locator(".ds-plugin-panel").getByTitle("打开目录").click();
  await page.waitForTimeout(400);
  const flat = JSON.stringify(calls);
  expect(flat).toContain('"profile":"web"');
  expect(flat).toContain('"version":"0.2.0-rc.1"');
});
