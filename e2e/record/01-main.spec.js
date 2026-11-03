import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, hover, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 01 主界面全貌：展示环境检查通过 + 已安装一个版本 + 空的可选版本列表。
test("01 主界面", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: "0.2.1-rc.2",
      isolated_versions: [],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [
      makeVersion("0.2.1-rc.2", { isDefault: true, installed_at: "2026-10-01" }),
    ],
  });
  await page.goto("/");
  await waitReady(page);
  // 停在"运行"按钮上，展示 v0.3.4 的 hover 动效
  await hover(page, page.getByRole("button", { name: "运行", exact: true }).first());
  await page.waitForTimeout(600);
  await page.waitForTimeout(HOLD_END);
});
