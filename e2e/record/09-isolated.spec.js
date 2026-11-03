import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 09 隔离状态与「已隔离 ▾」菜单：隔离版本有专属菜单，点开后展示五项操作。
test("09 隔离菜单", async ({ page }) => {
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
    ],
    extra: {
      scan_isolated_size: { total: 339294618 },
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 点第二个版本的「已隔离 ▾」
  const menuBtn = page.getByRole("button", { name: /已隔离/ }).first();
  await clickAt(page, menuBtn);
  await page.waitForTimeout(600);

  // 悬停「清理隔离数据」（danger 项），展示危险态
  const danger = page.locator(".menu-item.danger").first();
  if (await danger.count()) {
    const db = await danger.boundingBox();
    if (db) {
      await glide(page, db.x + db.width / 2, db.y + db.height / 2);
      await page.waitForTimeout(700);
    }
  }
  await page.waitForTimeout(HOLD_END);
});
