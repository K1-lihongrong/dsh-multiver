import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, hover, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 05 可用版本列表：对应正文 180-185 行。
// "支持搜索版本号"
// "支持排序（最新在前 / 最早在前）"
// "点版本号填入输入框，同时在其下方就地展开该版本的更新说明"
// "标灰划线的版本是「已知安装失败」的，可以点「清除标记」后重试"
test("05 可用版本列表", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: null,
      isolated_versions: [],
      broken_versions: ["0.1.5-rc.3"],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" })],
    extra: {
      fetch_release_note: {
        version: "0.2.1-rc.2",
        body: "## 本次更新\n- 修复插件面板在窄窗口下的溢出\n- 安装失败时按错误类别给出针对性提示\n- 内嵌窗口顶栏圆点支持一键重启",
        html_url: "https://github.com/deepseek-ai/dsh/releases",
      },
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 1) 点「刷新列表」拉取远端版本（正文 180 行前提）
  cue(
    "可用版本列表：从 npm 拉取所有可安装的 dsh 版本，最新在前",
    "Available versions: all installable dsh versions fetched from npm, newest first",
    "利用可能なバージョン一覧：npm から取得したインストール可能な dsh の全バージョン（新しい順）",
  );
  await clickAt(page, page.getByRole("button", { name: /刷新列表/ }).first());
  await page.waitForTimeout(900);

  // 2) 搜索版本号（正文 180 行）
  cue(
    "支持按版本号搜索筛选",
    "Supports filtering by version number",
    "バージョン番号での絞り込み検索に対応",
  );
  const search = page.locator(".remote-search").first();
  if (await search.count()) {
    const box = await search.boundingBox();
    if (box) {
      await glide(page, box.x + 40, box.y + box.height / 2);
      await page.mouse.click(box.x + 40, box.y + box.height / 2);
      await page.waitForTimeout(200);
      await page.keyboard.type("0.1", { delay: 100 });
      await page.waitForTimeout(700);
      await page.keyboard.press("Control+A");
      await page.keyboard.press("Backspace");
      await page.waitForTimeout(400);
    }
  }

  // 3) 切换排序：最新在前 → 最早在前（正文 181 行）
  cue(
    "支持排序：最新在前 / 最早在前",
    "Sorting: newest first or oldest first",
    "並び替え：新しい順 / 古い順",
  );
  const sortBtn = page.locator(".remote-toolbar button").first();
  if (await sortBtn.count()) {
    await clickAt(page, sortBtn);
    await page.waitForTimeout(700);
    await clickAt(page, sortBtn);
    await page.waitForTimeout(700);
  }

  // 4) 点一个版本 chip：填入输入框 + 展开更新说明（正文 182 行）
  cue(
    "点版本号填入输入框，同时在其下方就地展开该版本的更新说明；再点一次收起",
    "Click a version to fill the input and expand its release notes inline; click again to collapse",
    "バージョン番号をクリックすると入力欄に反映され、その下に更新情報が展開されます。もう一度クリックで折りたたみ",
  );
  const chip = page.locator(".chip").first();
  if (await chip.count()) {
    await clickAt(page, chip);
    await page.waitForTimeout(1200);
  }

  // 5) 悬停标灰版本，展示 tooltip（正文 185 行）
  cue(
    "标灰划线的版本是「已知安装失败」的（依赖已下架的私有包等），可点「清除标记」后重试",
    "Greyed-out versions are known to fail installing (e.g. removed private dependencies); click \"Clear mark\" to retry",
    "グレー表示のバージョンは「インストール失敗既知」です（削除されたプライベートパッケージなど）。「マークをクリア」で再試行できます",
  );
  const broken = page.locator(".chip-broken").first();
  if (await broken.count()) {
    const bb = await broken.boundingBox();
    if (bb) {
      await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
      await page.waitForTimeout(900);
    }
  }
  saveCues("05-remote");
  await page.waitForTimeout(HOLD_END);
});
