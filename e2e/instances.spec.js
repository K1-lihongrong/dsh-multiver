import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion, makeInstance } from "./helpers.js";

const V = "0.2.1-rc.4";

function baseFixtures(extra = {}) {
  const v = makeVersion(V, { isolated: true });
  const i1 = makeInstance("i1", "默认隔离", { mounted_versions: [V] });
  const i2 = makeInstance("i2", "对比用", { mounted_versions: [V] });
  return {
    installed: [v],
    instances: [i1, i2],
    versionTargets: { [V]: "i1" },
    extra,
  };
}

test("实例浮层列出共享 home 与实例，选择写 selected_target", async ({ page }) => {
  await page.exposeFunction("__recordTarget", (args) => calls.push(args));
  const calls = [];
  await mockTauri(page, {
    ...baseFixtures(),
    extra: {
      set_selected_target: "(args) => { window.__recordTarget(args); return null; }",
    },
  });
  await page.goto("/");
  await page.locator(".run-caret").first().click();
  await expect(page.locator(".instance-menu")).toBeVisible();
  await expect(page.locator(".instance-menu")).toContainText("共享 home");
  await expect(page.locator(".instance-menu")).toContainText("对比用");
  await page
    .locator(".im-row")
    .filter({ hasText: "对比用" })
    .locator('input[type="radio"]')
    .check();
  await expect.poll(() => calls.length).toBe(1);
  expect(calls[0]).toMatchObject({ version: V, instance: "i2" });
});

test("主按钮按 selected_target 运行实例", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordRun", (args) => calls.push(args));
  await mockTauri(page, {
    ...baseFixtures(),
    extra: {
      run_version: "(args) => { window.__recordRun(args); return Promise.resolve('ok'); }",
    },
  });
  await page.goto("/");
  await expect(page.locator(".run-main").first()).toContainText("运行 默认隔离");
  await page.locator(".run-main").first().click();
  await expect.poll(() => calls.length).toBe(1);
  expect(calls[0]).toMatchObject({ version: V, instance: "i1" });
});

test("实例在其他版本运行时分隔显示运行中与聚焦", async ({ page }) => {
  const i2 = makeInstance("i2", "对比用", { mounted_versions: [V] });
  await mockTauri(page, {
    installed: [makeVersion(V, { isolated: true })],
    instances: [i2],
    versionTargets: { [V]: "i2" },
    runningTargets: [{ kind: "instance", version: "0.2.0", instance: "i2", name: "对比用" }],
  });
  await page.goto("/");
  await expect(page.locator(".run-main").first()).toContainText("聚焦 对比用");
  await page.locator(".run-caret").first().click();
  await expect(page.locator(".instance-menu")).toContainText("运行中（0.2.0）");
});

test("全局实例面板显示孤儿并禁用运行中删除", async ({ page }) => {
  const orphan = makeInstance("i9", "孤儿实例", { mounted_versions: [] });
  const running = makeInstance("i1", "默认隔离", {
    mounted_versions: [V],
    running_on: V,
  });
  await mockTauri(page, {
    installed: [makeVersion(V, { isolated: true })],
    instances: [running, orphan],
    versionTargets: { [V]: "i1" },
    runningTargets: [{ kind: "instance", version: V, instance: "i1", name: "默认隔离" }],
  });
  await page.goto("/");
  await page.locator(".instance-panel-btn").click();
  await expect(page.locator(".ip-modal")).toBeVisible();
  await expect(page.locator(".ip-row.orphan")).toContainText("孤儿实例");
  await expect(page.locator(".ip-row.orphan")).toContainText("未挂载");
  await expect(page.locator(".ip-row").filter({ hasText: "默认隔离" }).locator(".ip-delete")).toBeDisabled();
});

test("新建实例提交名称与挂载版本", async ({ page }) => {
  const calls = [];
  await page.exposeFunction("__recordCreate", (args) => calls.push(args));
  await mockTauri(page, {
    ...baseFixtures(),
    extra: {
      instance_create: "(args) => { window.__recordCreate(args); return Promise.resolve('i3'); }",
    },
  });
  await page.goto("/");
  await page.locator(".instance-panel-btn").click();
  await page.getByRole("button", { name: /新建实例/ }).click();
  await page.locator(".ip-input").first().fill("新实例");
  await page.locator(".ip-versions input[type=checkbox]").first().check();
  await page.locator(".ip-create-form").getByRole("button", { name: "创建", exact: true }).click();
  await expect.poll(() => calls.length).toBe(1);
  expect(calls[0]).toMatchObject({ name: "新实例", attach: [V] });
});
