import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, scrollTo, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 11 维护面板：滚动到路径设置里的维护区，展示开关、上次执行时间与三个按钮。
test("11 维护面板", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: null,
      isolated_versions: [],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: {
        auto_enabled: true,
        // fmtTs 期望 Unix 秒：2026-11-02 08:15 UTC / 2026-10-28 21:40 UTC
        last_cleanup_at: 1793607300,
        last_cleanup_count: 3,
        last_prune_at: 1793223600,
      },
    },
    installed: [makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" })],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 滚到维护区（面板底部）
  cue(
    "维护面板在「路径设置」底部",
    "The maintenance panel sits at the bottom of Path settings",
    "メンテナンスパネルは「パス設定」の最下部にあります",
  );
  await scrollTo(page, ".maint", -120);
  await page.waitForTimeout(700);

  // 正文 648-650 行：自动维护开关 + 规则说明 + 上次执行状态 + 三个手动按钮
  // 1) 演示开关：关 → 开（正文 648 行"关掉则启动时不做上述两件事"）
  cue(
    "自动维护开关：关掉则启动时不做孤立缓存清理与 store prune",
    "Auto maintenance toggle: turning it off skips orphan-cache cleanup and store prune on startup",
    "自動メンテナンスのスイッチ：オフにすると起動時の孤立キャッシュ削除と store prune を実行しません",
  );
  const toggle = page.locator(".maint .checkbox-row input[type=checkbox]").first();
  if (await toggle.count()) {
    await clickAt(page, toggle);
    await page.waitForTimeout(500);
    await clickAt(page, toggle);
    await page.waitForTimeout(500);
  }

  // 2) 依次悬停三个手动按钮（正文 650 行）
  cue(
    "三个手动按钮：清理孤立缓存 / 回收依赖仓库 / 打开日志目录；上方显示上次执行时间与结果",
    "Three manual actions: clean orphan cache, prune dependency store, open log folder. Last run time and result are shown above",
    "手動ボタンは 3 つ：孤立キャッシュを削除 / 依存ストアを整理 / ログフォルダを開く。上部に前回の実行時刻と結果が表示されます",
  );
  for (const name of [/清理孤立缓存/, /回收依赖仓库/, /打开日志目录/]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(750);
      }
    }
  }
  saveCues("11-maintenance");
  await page.waitForTimeout(HOLD_END);
});
