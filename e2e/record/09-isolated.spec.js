import { test } from "@playwright/test";
import { mockTauri, makeVersion, makeInstance } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 09 隔离状态与「已隔离 ▾」菜单：隔离版本有专属菜单，点开后展示五项操作。
test("09 隔离菜单", async ({ page }) => {
  startCues();
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
    instances: [
      makeInstance("i1", "默认隔离", { mounted_versions: ["0.2.0-rc.1"] }),
      makeInstance("i2", "对比用", { mounted_versions: ["0.2.0-rc.1"] }),
    ],
    versionTargets: { "0.2.0-rc.1": "i1" },
    extra: {
      scan_isolated_size: { total: 339294618 },
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  cue(
    "点「已隔离 ▾」菜单：隔离版本使用独立的 home，数据互不干扰",
    "Click the \"Isolated ▾\" menu: isolated versions use their own home directory, so data stays separate",
    "「隔離済み ▾」メニューをクリック：隔離バージョンは専用の home を使用し、データが互いに影響しません",
  );
  // 点第二个版本的分体按钮小三角
  const menuBtn = page.locator(".run-caret").nth(1);
  await clickAt(page, menuBtn);
  await page.waitForTimeout(600);

  // 正文 360-364 行列出 5 项操作，依次悬停让观众看清
  cue(
    "5 项操作：关闭隔离 / 打开隔离目录 / 扫描占用大小 / 复制共享数据到此 / 清理隔离数据",
    "Five actions: disable isolation, open isolated folder, scan size, copy shared data here, clear isolated data",
    "5 つの操作：隔離を解除 / 隔離フォルダを開く / サイズをスキャン / 共有データをここにコピー / 隔離データをクリア",
  );
  // 打开隔离目录 / 扫描占用大小 / 复制共享数据到此 / 清理隔离数据 / 关闭隔离
  const items = page.locator(".im-act");
  const count = await items.count();
  for (let i = 0; i < count; i++) {
    const ib = await items.nth(i).boundingBox();
    if (ib) {
      await glide(page, ib.x + ib.width / 2, ib.y + ib.height / 2);
      await page.waitForTimeout(650);
    }
  }
  // 停在最后一项「清理隔离数据」（danger 红字）上收尾
  const danger = page.locator(".im-act.danger").first();
  if (await danger.count()) {
    const db = await danger.boundingBox();
    if (db) {
      await glide(page, db.x + db.width / 2, db.y + db.height / 2);
      await page.waitForTimeout(900);
    }
  }
  saveCues("09-isolated");
  await page.waitForTimeout(HOLD_END);
});
