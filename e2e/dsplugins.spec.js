import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

// v0.3.0 双向桥 B1：dsh 插件管理面板。

const PROFILE = {
  name: "web",
  path: "D:/dsh/profiles/web",
  plugins: [
    {
      name: "@deepseek-ai/dsh-base",
      version: "0.2.1",
      is_bundle: true,
      source: "bundle",
      origin: "official",
      official: true,
      declares_bundle: true,
      description: "DSH base bundle",
    },
    {
      name: "my-plugin",
      version: "1.0.0",
      is_bundle: false,
      source: "user",
      origin: "third_party",
      official: false,
      declares_bundle: false,
      description: "third party plugin",
    },
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
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("x");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
  await page.waitForTimeout(400);
  await expect(page.locator(".dp-modal")).toContainText("版本不兼容");
  await expect(page.locator(".dp-modal")).toContainText("诊断日志");
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
  // 与官方语义对齐：官方随附 bundle 标「随 DSH 提供」，其余「已安装」；非 dsh.bundle 标「依赖」
  await expect(page.locator(".dp-badge-bundle")).toHaveText("随 DSH 提供");
  await expect(page.locator(".dp-badge-user")).toHaveText("已安装");
  await expect(page.locator(".dp-badge-kind")).toHaveText("依赖（非插件）");
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
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("test-pkg");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
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
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("bad-pkg");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
  await page.waitForTimeout(400);
  await expect(page.locator(".dp-modal")).toContainText("模拟安装失败");
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

test("显示待决定构建脚本并允许（B1-12）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordApprove", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_profile_info: {
        runtime_version: "0.2.0-rc.1",
        exemptions: {},
        pending_builds: ["esbuild"],
        logs_dir: "D:/dsh/profiles/web/.plugin-manager/logs",
      },
      ds_approve_builds: "(args) => { window.__recordApprove(args); return Promise.resolve([]); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await expect(page.locator(".dp-pending")).toContainText("esbuild");
  await page.getByRole("button", { name: "允许并重试", exact: true }).click();
  await page.waitForTimeout(400);
  const flatApprove = JSON.stringify(calls);
  expect(flatApprove).toContain("esbuild");
});

test("显示版本豁免并可撤销（B1-12）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordExemption", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_profile_info: {
        runtime_version: "0.2.0-rc.1",
        exemptions: { "pkg@1.0.0": ["0.2.0-rc.1"] },
        pending_builds: [],
        logs_dir: "D:/dsh/profiles/web/.plugin-manager/logs",
      },
      ds_exemption_action: "(args) => { window.__recordExemption(args); return Promise.resolve('ok'); }",
      "plugin:dialog|message": "Yes",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.locator(".dp-exemptions summary").click();
  await expect(page.getByText("pkg@1.0.0")).toBeVisible();
  await page.getByRole("button", { name: "撤销", exact: true }).click();
  await page.waitForTimeout(400);
  const flatEx = JSON.stringify(calls);
  expect(flatEx).toContain('"action":"revoke"');
  expect(flatEx).toContain("pkg@1.0.0");
});

test("安装对话框选择镜像源 → registry 透传（对齐官方安装源）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordRegistry", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_action: "(args) => { window.__recordRegistry(args); return Promise.resolve('ok'); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("test-pkg");
  await page.locator(".dp-modal select").selectOption("mirror");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
  await page.waitForTimeout(400);
  const flat = JSON.stringify(calls);
  expect(flat).toContain("registry.npmmirror.com");
});

test("检查更新 → 显示新版本并可更新（B1-13）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordUpdate", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_check_updates: [{ name: "my-plugin", current: "1.0.0", latest: "2.0.0" }],
      ds_plugin_action: "(args) => { window.__recordUpdate(args); return Promise.resolve('ok'); }",
      "plugin:dialog|message": "Yes",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: "检查更新", exact: true }).click();
  await expect(page.locator(".dp-badge-update")).toContainText("2.0.0");
  await page.getByRole("button", { name: "更新", exact: true }).click();
  await page.waitForTimeout(500);
  const flat = JSON.stringify(calls);
  expect(flat).toContain('"action":"remove"');
  expect(flat).toContain("my-plugin@2.0.0");
});

test("允许版本豁免（B1-14）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordAllow", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_profile_info: {
        runtime_version: "0.2.0-rc.1",
        exemptions: {},
        pending_builds: [],
        logs_dir: "D:/dsh/profiles/web/.plugin-manager/logs",
      },
      ds_exemption_action: "(args) => { window.__recordAllow(args); return Promise.resolve('ok'); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.locator(".dp-exemptions summary").click();
  await page.locator(".dp-allow-spec").fill("pkg@1.0.0");
  await page.locator(".dp-allow-rt").fill("0.2.0-rc.1");
  await page.locator(".dp-allow-risk input").check();
  await page.getByRole("button", { name: "允许此版本", exact: true }).click();
  await page.waitForTimeout(400);
  const flat = JSON.stringify(calls);
  expect(flat).toContain('"action":"allow"');
  expect(flat).toContain("pkg@1.0.0");
});
