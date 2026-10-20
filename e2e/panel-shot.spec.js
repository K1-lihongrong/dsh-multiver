import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

test("插件面板截图", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockTauri(page, {
    installed: [makeVersion("0.2.1-rc.2", { isolated: true })],
    extra: {
      ds_list_plugins: {
        dsh_home: "E:\\.LLM-AGENTS\\deepseek-harness\\home",
        home_kind: "shared",
        profiles: [{
          name: "web", path: "x",
          plugins: [
            { name: "@deepseek-ai/dsh-base", version: "0.2.1", is_bundle: true },
            { name: "@deepseek-ai/dsh-web-app", version: "0.2.1", is_bundle: true },
            { name: "@deepseek-ai/dsh-experimental-auto-review", version: "0.2.1", is_bundle: true },
            { name: "@scope/my-plugin", version: "1.0.0", is_bundle: false },
          ],
        }],
      },
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: "插件", exact: true }).click();
  await page.waitForTimeout(500);
  await page.screenshot({ path: "test-results/plugin-panel.png", fullPage: false });
});
