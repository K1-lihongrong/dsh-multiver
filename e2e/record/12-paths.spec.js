import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, scrollTo, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 12 路径设置：展示根目录输入框、目录列表，点「展开全部目录」展示额外目录。
test("12 路径设置", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      home_dir: "D:/dsh-data/home",
      store_dir: "D:/dsh-data/store",
      cache_dir: "D:/dsh-data/cache",
      state_dir: "D:/dsh-data/state",
      extra_dirs: [
        { name: "logs", desc: "启动错误与维护日志", path: "D:/dsh-data/logs" },
        { name: "webview", desc: "各版本内嵌窗口数据", path: "D:/dsh-data/webview" },
        { name: "trash", desc: "卸载回收站", path: "D:/dsh-data/trash" },
        { name: "assets", desc: "内部资源（快捷方式图标）", path: "D:/dsh-data/assets" },
      ],
      default_version: null,
      isolated_versions: [],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" })],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 滚到路径设置区
  cue(
    "路径设置：可修改数据根目录，改完会同步更新终端 dsh 脚本里的路径",
    "Path settings: you can change the data root; the terminal dsh script is updated accordingly",
    "パス設定：データルートを変更できます。変更するとターミナルの dsh スクリプト内のパスも更新されます",
  );
  await scrollTo(page, "#path-settings", -80);
  await page.waitForTimeout(800);

  // 点「展开全部目录」
  cue(
    "默认展示 5 个核心目录；点「展开全部目录」可看到 logs / webview / trash / assets",
    "Five core directories are shown by default; click \"Show all directories\" to reveal logs, webview, trash and assets",
    "既定では主要 5 ディレクトリを表示。「すべてのディレクトリを表示」で logs / webview / trash / assets も確認できます",
  );
  const expand = page.getByRole("button", { name: /展开全部目录/ }).first();
  if (await expand.count()) {
    await clickAt(page, expand);
    await page.waitForTimeout(800);
  }

  // 悬停目录行，展示"点击即在文件管理器打开"（正文 215 行）
  cue(
    "点击任意目录行，即在文件管理器打开该目录",
    "Click any directory row to open it in the file manager",
    "任意のディレクトリ行をクリックすると、ファイルマネージャーで開きます",
  );
  for (const idx of [1, 2]) {
    const item = page.locator(".path-item").nth(idx);
    if (await item.count()) {
      const ib = await item.boundingBox();
      if (ib) {
        await glide(page, ib.x + ib.width / 2, ib.y + ib.height / 2);
        await page.waitForTimeout(650);
      }
    }
  }

  // 正文 221 行：「导出配置」和「导入配置」按钮
  cue(
    "「导出配置」把当前配置与版本清单存为 JSON；「导入配置」选备份文件后预览改动再应用",
    "\"Export config\" saves the current settings and version list as JSON; \"Import config\" previews changes before applying",
    "「設定をエクスポート」は現在の設定とバージョン一覧を JSON に保存します。「設定をインポート」はバックアップを選択し、変更点を確認してから適用します",
  );
  for (const name of [/导出配置/, /导入配置/]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(750);
      }
    }
  }
  saveCues("12-paths");
  await page.waitForTimeout(HOLD_END);
});
