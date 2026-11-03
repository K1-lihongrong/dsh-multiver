import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

const PROFILE = {
  name: "web", path: "D:/dsh/profiles/web",
  plugins: [
    { name: "@deepseek-ai/dsh-base", version: "0.2.1", is_bundle: true, source: "bundle", origin: "official", official: true, declares_bundle: true, description: "DSH base bundle" },
    { name: "my-plugin", version: "1.0.0", is_bundle: false, source: "user", origin: "third_party", official: false, declares_bundle: false, description: "third party plugin" },
  ],
};

test("D22 插件按钮展开态与收起态视觉区分", async ({ page }) => {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
    },
  });
  await page.goto("/");

  const btn = page.getByRole("button", { name: "插件", exact: true });
  await expect(btn).toBeVisible();

  // 收起态：无 is-open、aria-expanded=false
  await expect(btn).not.toHaveClass(/is-open/);
  await expect(btn).toHaveAttribute("aria-expanded", "false");

  // 展开：按钮高亮 + aria-expanded=true + 面板可见
  await btn.click();
  await page.mouse.move(0, 0);   // 移开鼠标，避开 :hover 覆盖展开态色值
  await expect(page.locator(".ds-plugin-panel")).toBeVisible();
  await expect(btn).toHaveClass(/is-open/);
  await expect(btn).toHaveAttribute("aria-expanded", "true");

  // 用 toHaveCSS 断言（自动重试到过渡结束）
  // --accent-weak #eef1ff = rgb(238, 241, 255)；--accent #4f6ef7 = rgb(79, 110, 247)
  await expect(btn).toHaveCSS("background-color", "rgb(238, 241, 255)");
  await expect(btn).toHaveCSS("color", "rgb(79, 110, 247)");

  // 再点一次收起：恢复默认外观
  await btn.click();
  await page.mouse.move(0, 0);
  await expect(page.locator(".ds-plugin-panel")).toHaveCount(0);
  await expect(btn).not.toHaveClass(/is-open/);
  await expect(btn).toHaveAttribute("aria-expanded", "false");
  await expect(btn).toHaveCSS("color", "rgb(55, 65, 81)");   // --text-2 #374151
});
