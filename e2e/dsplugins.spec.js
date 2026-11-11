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
//
// 注意：dsh CLI 实际没有 allow-version 子命令（plugin 只是转发给 pnpm），
// 因此不再把错误里的 allow-version 转成"可执行指引"，只透传原文 + 诊断日志路径。
test("安装失败且带官方诊断 → 透传原文并提示诊断日志", async ({ page }) => {
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
  await expect(page.locator(".dp-modal")).toContainText("incompatible");
  await expect(page.locator(".dp-modal")).toContainText("诊断日志");
});


test("共享 home 体检 → 显示旧版本残留报告", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_shared_home_health: {
        home: "D:/dsh",
        current_version: "0.2.0-rc.1",
        summary: { scanned: 3, stale: 1, dangling: 0, duplicate: 1 },
        issues: [
          { kind: "stale_junction", package: "@deepseek-ai/dsh-agent-presets", source_version: "0.0.1-rc.5", target: "D:/dsh/versions/0.0.1-rc.5/node_modules/x", detail: "链接来自 DSH 0.0.1-rc.5" },
        ],
      },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /共享 home 体检/ }).click();
  await expect(page.locator(".dp-health-modal")).toBeVisible();
  await expect(page.locator(".dp-health-modal")).toContainText("旧版本 1");
  await expect(page.locator(".dp-health-modal")).toContainText("@deepseek-ai/dsh-agent-presets");
});

test("安装失败分类 → 保留原始输出并显示动作", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_action: "(args) => Promise.reject('EPERM: file is being used by another process')",
      ds_classify_plugin_failure: "(args) => Promise.resolve({ code: 'file_locked', message: '文件被占用', detail: '关闭 dsh 后重试', raw: args.output, log_path: null })",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("x");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
  await page.waitForTimeout(500);
  await expect(page.locator(".dp-failure-card")).toContainText("文件被占用");
  await expect(page.locator(".dp-modal-log")).toContainText("EPERM");
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
  await page.getByPlaceholder("筛选已安装的插件…").fill("my-plugin");
  await expect(page.locator(".dp-item", { hasText: "my-plugin" })).toBeVisible();
  await expect(page.locator(".dp-item", { hasText: "@deepseek-ai/dsh-base" })).toHaveCount(0);
});

test("无已安装插件时不显示筛选框", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [{ name: "web", path: "D:/p", plugins: [] }] },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await expect(page.locator(".dp-state-empty")).toBeVisible();
  // 空列表下筛选框无意义，不应出现
  await expect(page.locator(".dp-filter")).toHaveCount(0);
  await expect(page.getByPlaceholder("筛选已安装的插件…")).toHaveCount(0);
});

test("有插件时筛选框位于列表上方（与工具栏分离）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: { ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] } },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  // 筛选框存在，且不在工具栏内
  await expect(page.locator(".dp-filter .dp-search")).toBeVisible();
  await expect(page.locator(".dp-toolbar .dp-search")).toHaveCount(0);
  await expect(page.locator(".dp-filter-icon")).toBeVisible();
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
  await page.locator(".dp-pending").getByRole("button", { name: "允许并重试", exact: true }).click();
  await page.waitForTimeout(400);
  const flatApprove = JSON.stringify(calls);
  expect(flatApprove).toContain("esbuild");
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
      ds_plugin_update_batch:
        "(args) => { window.__recordUpdate(args); return Promise.resolve([{name:'my-plugin',current:'1.0.0',latest:'2.0.0',status:'updated',message:null,rollback_error:null}]); }",
      "plugin:dialog|message": "Yes",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: "检查更新", exact: true }).click();
  await expect(page.locator(".dp-badge-update")).toContainText("2.0.0");
  await page.getByRole("button", { name: "更新", exact: true }).click();
  await page.waitForTimeout(500);
  expect(calls).toHaveLength(1);
  expect(calls[0].targets).toEqual([{ name: "my-plugin", latest: "2.0.0" }]);
});

test("批量更新 → 一次提交全部目标并回显失败回滚", async ({ page }) => {
  const calls = [];
  const batchProfile = {
    name: "web",
    path: "D:/dsh/profiles/web",
    plugins: [
      {
        name: "p1",
        version: "1.0.0",
        is_bundle: true,
        source: "user",
        origin: "third_party",
        official: false,
        declares_bundle: true,
        description: "p1",
      },
      {
        name: "p2",
        version: "1.0.0",
        is_bundle: false,
        source: "user",
        origin: "third_party",
        official: false,
        declares_bundle: false,
        description: "p2",
      },
    ],
  };
  await page.exposeFunction("__recordBatchUpdate", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [batchProfile] },
      ds_check_updates: [
        { name: "p1", current: "1.0.0", latest: "2.0.0" },
        { name: "p2", current: "1.0.0", latest: "2.0.0" },
      ],
      ds_plugin_update_batch:
        "(args) => { window.__recordBatchUpdate(args); return Promise.resolve([{name:'p1',current:'1.0.0',latest:'2.0.0',status:'updated',message:null,rollback_error:null},{name:'p2',current:'1.0.0',latest:'2.0.0',status:'rolled_back',message:'更新失败，已回滚到 1.0.0：boom',rollback_error:null}]); }",
      "plugin:dialog|message": "Yes",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: "检查更新", exact: true }).click();
  await expect(page.getByRole("button", { name: /全部更新/ })).toContainText("2");
  await page.getByRole("button", { name: /全部更新/ }).click();
  await page.waitForTimeout(800);
  expect(calls).toHaveLength(1);
  expect(calls[0].targets).toEqual([
    { name: "p1", latest: "2.0.0" },
    { name: "p2", latest: "2.0.0" },
  ]);
  await expect(page.locator(".dp-log-body")).toContainText("已回滚到 1.0.0");
  await expect(page.getByRole("button", { name: /添加插件/ })).toBeEnabled();
  await expect(page.getByRole("button", { name: "检查更新", exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: /共享 home 体检/ })).toBeEnabled();
});

test("安装遇到不兼容 → 一键精确版本豁免并重试", async ({ page }) => {
  const exemptions = [];
  await page.exposeFunction("__recordExemption", (x) => exemptions.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_action:
        "() => { window.__attempt = (window.__attempt || 0) + 1; if (window.__attempt === 1) return Promise.reject('Plugin @deepseek-ai/test-plugin@1.0.0 is incompatible with dsh 0.2.2-rc.1: requirements {}'); return Promise.resolve('ok'); }",
      ds_exemption_action:
        "(args) => { window.__recordExemption(args); return Promise.resolve('dsh: allowed'); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("@deepseek-ai/test-plugin");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
  await expect(page.locator(".dp-incompatible")).toContainText("精确版本");
  await page.getByRole("button", { name: /允许全部 1 个并重试/ }).click();
  await page.waitForTimeout(700);
  expect(exemptions).toHaveLength(1);
  expect(exemptions[0]).toMatchObject({
    action: "allow",
    package_version: "@deepseek-ai/test-plugin@1.0.0",
    dsh_version: "0.2.2-rc.1",
  });
  await expect(page.locator(".dp-done-title")).toContainText("安装完成");
});

test("安装前 inspect 拒绝非组合包（B1-16）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordInspectInstall", (x) => calls.push(x));
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_inspect_spec: {
        kind: "npm",
        name: "plain-pkg",
        version: "1.0.0",
        description: "",
        declares_bundle: false,
        problem: "not-a-bundle",
        detail: null,
      },
      ds_plugin_action: "(args) => { window.__recordInspectInstall(args); return Promise.resolve('ok'); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("plain-pkg");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.waitForTimeout(400);
  await expect(page.locator(".dp-modal")).toContainText("不是组合包插件");
  expect(JSON.stringify(calls)).not.toContain("plain-pkg");
});

test("首次打开安装对话框 → 探测最快源（B1-17）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_probe_registries: { kind: "mirror", url: "https://registry.npmmirror.com", ms: 120 },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.waitForTimeout(300);
  await expect(page.locator(".dp-modal select")).toHaveValue("mirror");
  await expect(page.locator(".dp-modal")).toContainText("已探测最快安装源");
});

test("失败弹窗内允许构建脚本并重试（B1-12 对齐官方）", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordBuildRetry", (x) => calls.push(x));
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
      ds_approve_builds: "(args) => { window.__recordBuildRetry({kind:'approve', args}); return Promise.resolve([]); }",
      ds_plugin_action:
        "(args) => { window.__recordBuildRetry({kind:'action', args}); return Promise.reject('Ignored build scripts: esbuild'); }",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("needs-build");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
  await page.waitForTimeout(400);
  await page.locator(".dp-modal").getByRole("button", { name: "允许并重试", exact: true }).click();
  await page.waitForTimeout(500);
  const flat = JSON.stringify(calls);
  expect(flat).toContain('"kind":"approve"');
  expect(flat).toContain("esbuild");
});

test("安装中关闭对话框 → 推荐页安装按钮仍禁用", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_catalog: {
        plugins: [
          { name: "some-pkg", description: "示例包", category: "工具", repo: "https://github.com/x/y", tags: [] },
        ],
      },
      // 永不 resolve：安装一直停在 installing 阶段
      ds_plugin_action: "() => new Promise(() => {})",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("button", { name: /添加插件/ }).click();
  await page.locator(".dp-modal-input").first().fill("hang-pkg");
  await page.locator(".dp-modal").getByRole("button", { name: "安装", exact: true }).click();
  await page.locator(".dp-modal").getByRole("button", { name: "确认安装", exact: true }).click();
  await page.waitForTimeout(300);
  // 安装卡在 installing：此时仍可关闭对话框（closeInstall 不判 installing）
  await page.locator(".dp-modal-head .dp-icon-btn").click();
  await expect(page.locator(".dp-modal")).toHaveCount(0);
  // 切到推荐页：安装按钮应保持禁用——installing 状态由壳持有，
  // 依赖子组件 emit("installing-change")，漏声明时此处会失败。
  await page.getByRole("tab", { name: "推荐" }).click();
  await expect(page.locator(".dp-cc-install")).toBeDisabled();
});

// ---------- 推荐页（v0.3.3 补：此前完全无覆盖）----------

/// 推荐页的 catalog mock 数据。三条，两个分类，用于筛选测试。
const CATALOG = {
  plugins: [
    { name: "pkg-a", description: "工具 A 的插件", category: "工具增强", repo: "https://github.com/x/a" },
    { name: "pkg-b", description: "内容聚合的插件", category: "内容聚合", repo: "https://github.com/x/b" },
    { name: "pkg-c", description: "另一个工具插件", category: "工具增强", repo: "https://github.com/x/c" },
  ],
};

/// 打开插件面板并切到「推荐」标签。各条推荐页用例共用。
async function openCatalog(page) {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
    },
  });
}

test("推荐页：分类标签筛选条目", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_catalog: CATALOG,
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("tab", { name: "推荐" }).click();
  await expect(page.locator(".dp-catalog-card")).toHaveCount(3);

  // 分类栏应含「全部 / 工具增强 / 内容聚合」
  const catTab = (label) => page.locator(".dp-cat-tab", { hasText: label });
  await expect(catTab("工具增强")).toBeVisible();
  await expect(catTab("内容聚合")).toBeVisible();

  await catTab("工具增强").click();
  await expect(page.locator(".dp-catalog-card")).toHaveCount(2);
  await expect(page.locator(".dp-cc-name").first()).toHaveText("pkg-a");

  await catTab("内容聚合").click();
  await expect(page.locator(".dp-catalog-card")).toHaveCount(1);
  await expect(page.locator(".dp-cc-name")).toHaveText("pkg-b");

  await catTab("全部").click();
  await expect(page.locator(".dp-catalog-card")).toHaveCount(3);
});

test("推荐页：星标条目排在最前且保持原相对顺序", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_catalog: {
        plugins: [
          { name: "pkg-normal", description: "普通", category: "工具增强", repo: "https://github.com/x/n" },
          { name: "pkg-featured", description: "推荐", category: "工具增强", repo: "https://github.com/x/f", tags: ["featured"] },
          { name: "pkg-author", description: "同作者", category: "工具增强", repo: "https://github.com/x/a", tags: ["same-author"] },
        ],
      },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("tab", { name: "推荐" }).click();

  const cards = page.locator(".dp-catalog-card");
  await expect(cards).toHaveCount(3);
  // 稳定排序：星标在前，两个星标保持原顺序（featured 在 same-author 前）
  await expect(cards.nth(0).locator(".dp-cc-name")).toHaveText("pkg-featured");
  await expect(cards.nth(1).locator(".dp-cc-name")).toHaveText("pkg-author");
  await expect(cards.nth(2).locator(".dp-cc-name")).toHaveText("pkg-normal");
  // 星标只在带 tags 的两条上出现
  await expect(cards.nth(0).locator(".dp-cc-star")).toBeVisible();
  await expect(cards.nth(1).locator(".dp-cc-star")).toBeVisible();
  await expect(cards.nth(2).locator(".dp-cc-star")).toHaveCount(0);
});

test("推荐页：点「查看完整描述」弹出浮窗显示全文", async ({ page }) => {
  const longDesc = "这是一段很长的插件描述，用来验证浮窗会显示完整内容而不是被截断。".repeat(4);
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_catalog: {
        plugins: [
          { name: "pkg-long", description: longDesc, category: "工具增强", repo: "https://github.com/x/long" },
        ],
      },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("tab", { name: "推荐" }).click();
  await expect(page.locator(".dp-catalog-card")).toHaveCount(1);

  await page.locator(".dp-cc-more").click();
  const tip = page.locator(".dp-cc-tip");
  await expect(tip).toBeVisible();
  await expect(tip).toHaveText(longDesc);
});

test("推荐页：描述随语言切换（descriptionI18n）", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_catalog: {
        plugins: [
          {
            name: "pkg-i18n",
            description: "中文描述",
            descriptionI18n: { "en-US": "English description", "ja-JP": "日本語の説明" },
            category: "工具增强",
            repo: "https://github.com/x/i18n",
          },
        ],
      },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.getByRole("tab", { name: "推荐" }).click();
  await expect(page.locator(".dp-cc-desc")).toHaveText("中文描述");

  // 通过界面切到 English（不能用 localStorage + reload：helpers 的
  // addInitScript 每次导航都会把 lang 重置回 zh-CN）
  await page.locator(".field-row select").last().selectOption("en-US");
  await expect(page.locator(".dp-cc-desc")).toHaveText("English description");

  // 切到日本语
  await page.locator(".field-row select").last().selectOption("ja-JP");
  await expect(page.locator(".dp-cc-desc")).toHaveText("日本語の説明");

  // 没有 descriptionI18n 的语言回退到中文原文（切回简中验证）
  await page.locator(".field-row select").last().selectOption("zh-TW");
  await expect(page.locator(".dp-cc-desc")).toHaveText("中文描述");
});
