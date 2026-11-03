import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 03 已安装版本（含占用复用/独占）：点「扫描全部占用」，展示两段式进度与结果。
test("03 已安装版本与占用", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: "0.2.1-rc.2",
      isolated_versions: ["0.2.0-rc.1"],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [
      makeVersion("0.2.1-rc.2", { isDefault: true, installed_at: "2026-10-01" }),
      makeVersion("0.2.0-rc.1", { isolated: true, installed_at: "2026-09-28" }),
      makeVersion("0.1.8", { installed_at: "2026-09-20" }),
    ],
    extra: {
      scan_usage: {
        "0.2.1-rc.2": { total: 1483237376, shared_size: 1181116006, exclusive_size: 302121370, shared_count: 12045 },
        "0.2.0-rc.1": { total: 1520410624, shared_size: 1181116006, exclusive_size: 339294618, shared_count: 12088 },
        "0.1.8": { total: 1398472704, shared_size: 1181116006, exclusive_size: 217356698, shared_count: 11902 },
      },
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);
  // 点「扫描全部占用」，等结果出现
  const btn = page.getByRole("button", { name: /扫描全部占用/ }).first();
  await clickAt(page, btn);
  await page.waitForTimeout(1500);
  await page.waitForTimeout(HOLD_END);
});
