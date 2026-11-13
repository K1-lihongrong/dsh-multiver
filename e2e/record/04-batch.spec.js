import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";
import { T } from "./i18n.js";

// 04 批量操作栏：对应正文 169-174 行。
// "每个版本左侧有复选框，勾选后底部出现批量操作栏"
// "「全选」选中本组，「清除选择」取消"
// "「批量卸载」一次确认后逐个执行"
test("04 批量操作栏", async ({ page }) => {
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
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [
      makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" }),
      makeVersion("0.2.0-rc.1", { installed_at: "2026-09-28" }),
      makeVersion("0.1.8", { installed_at: "2026-09-20" }),
    ],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  const boxes = page.locator(".ver-item .sel-box");

  // 1) 逐个勾选前两个版本，批量栏出现（正文 171 行）
  cue(
    "勾选版本左侧复选框，底部出现批量操作栏",
    "Tick the checkboxes on the left; the batch action bar appears at the bottom",
    "各バージョン左側のチェックボックスをオンにすると、下部に一括操作バーが表示されます",
  );
  await clickAt(page, boxes.nth(0));
  await page.waitForTimeout(250);
  await clickAt(page, boxes.nth(1));
  await page.waitForTimeout(500);

  // 2) 点「全选」（正文 172 行）→ 三个都选中
  cue(
    "「全选」一次选中本组全部版本；「清除选择」取消全部勾选",
    "\"Select all\" ticks every version in the group; \"Clear selection\" unticks them all",
    "「すべて選択」でグループ内の全バージョンを選択し、「選択をクリア」で全解除できます",
  );
  const selectAll = page.locator(".batch-select-all input").first();
  if (await selectAll.count()) {
    await clickAt(page, selectAll);
    await page.waitForTimeout(500);
  }

  // 3) 点「清除选择」（正文 172 行）→ 全部取消
  const clear = page.getByRole("button", { name: T("installed.clearSelection") }).first();
  if (await clear.count()) {
    await clickAt(page, clear);
    await page.waitForTimeout(400);
  }

  // 4) 重新勾选两个，悬停「批量卸载」展示危险按钮（正文 173 行）
  cue(
    "「批量卸载」一次确认后逐个执行，实时显示进度；危险按钮悬停时变为实心红底白字",
    "\"Batch uninstall\" runs one by one after a single confirmation with live progress; the danger button turns solid red on hover",
    "「一括アンインストール」は一度の確認で順番に実行され、進捗がリアルタイムで表示されます。危険ボタンはホバー時に赤く塗りつぶされます",
  );
  await clickAt(page, boxes.nth(0));
  await page.waitForTimeout(200);
  await clickAt(page, boxes.nth(2));
  await page.waitForTimeout(500);
  const danger = page.getByRole("button", { name: T("installed.batchUninstall") }).first();
  if (await danger.count()) {
    const box = await danger.boundingBox();
    if (box) {
      await glide(page, box.x + box.width / 2, box.y + box.height / 2);
      await page.waitForTimeout(800);
    }
  }
  saveCues("04-batch");
  await page.waitForTimeout(HOLD_END);
});
