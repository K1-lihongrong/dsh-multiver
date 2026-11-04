import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import {
  installCursor, glide, clickAt, hover, scrollTo, waitReady,
  startCues, cue, saveCues, HOLD_START, HOLD_END,
} from "./_rec.js";

// 13 插件面板 · 已安装标签：打开面板 → 看 profile 与插件列表 → 搜包名 → 悬停卸载 → 检查更新
const PROFILE = {
  name: "web",
  path: "D:/dsh/profiles/web",
  plugins: [
    {
      name: "@deepseek-ai/dsh-base",
      version: "0.2.1",
      is_bundle: true,
      source: "bundle",
      origin: "official",
      official: true,
      declares_bundle: true,
      description: "DSH base bundle",
    },
    {
      name: "dsh-plugin-memory",
      version: "1.2.0",
      is_bundle: false,
      source: "user",
      origin: "third_party",
      official: false,
      declares_bundle: false,
      description: "长期记忆插件",
    },
    {
      name: "dsh-plugin-websearch",
      version: "0.9.3",
      is_bundle: false,
      source: "user",
      origin: "third_party",
      official: false,
      declares_bundle: false,
      description: "联网搜索插件",
    },
  ],
};

test("13 插件面板 · 已安装", async ({ page }) => {
  startCues();
  await installCursor(page);

  await mockTauri(page, {
    installed: [makeVersion("0.2.1-rc.2", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      // 检查更新返回：memory 有新版
      ds_plugin_check_updates: {
        "dsh-plugin-memory": "1.3.0",
      },
    },
  });

  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 1) 点「插件」展开面板
  cue(
    "点某个版本的「插件」按钮，展开该版本的插件面板",
    "Click the \"Plugins\" button on a version to open its plugin panel",
    "バージョンの「プラグイン」ボタンをクリックすると、そのバージョンのプラグインパネルが開きます",
  );
  await scrollTo(page, ".ver-list", -120);
  const pluginBtn = page.getByRole("button", { name: "插件", exact: true }).first();
  await clickAt(page, pluginBtn);
  await page.locator(".dp-head").waitFor({ state: "visible", timeout: 8000 });
  await page.waitForTimeout(1200);

  // 2) 面板结构：DSH_HOME 来源 + 已安装列表
  cue(
    "面板顶部显示 dsh 数据目录来源；「已安装」列出该 profile 下所有插件，bundle 是内置层",
    "The panel shows the dsh data directory source; \"Installed\" lists every plugin in this profile, with bundles as the built-in layer",
    "パネル上部に dsh データディレクトリの由来を表示。「インストール済み」にはこのプロファイルの全プラグインが並び、bundle は内蔵レイヤーです",
  );
  await glide(page, 400, 300);
  await page.waitForTimeout(1400);

  // 3) 搜索过滤
  cue(
    "顶部搜索框可按包名过滤，插件多时快速定位",
    "The search box filters by package name — handy when the list grows",
    "上部の検索ボックスでパッケージ名による絞り込みができ、プラグインが多いときもすぐ見つかります",
  );
  const search = page.locator(".dp-search").first();
  if (await search.count()) {
    await clickAt(page, search);
    await page.keyboard.type("mem", { delay: 120 });
    await page.waitForTimeout(900);
    // 清空
    await page.keyboard.press("Control+A");
    await page.keyboard.press("Delete");
    await page.waitForTimeout(600);
  }

  // 4) 悬停卸载按钮（危险操作红底）
  cue(
    "鼠标悬停「卸载」按钮时变为实心红底白字，与普通按钮区分；卸载大插件会显示转圈和已运行秒数",
    "Hovering \"Uninstall\" turns it solid red — distinct from normal buttons. Large plugins show a spinner and elapsed seconds while removing",
    "「アンインストール」にカーソルを合わせると赤い塗りつぶしに変わり、通常のボタンと区別できます。大きなプラグインは削除中にスピナーと経過秒数を表示します",
  );
  const uninstall = page.getByRole("button", { name: "卸载", exact: true }).last();
  if (await uninstall.count()) {
    await hover(page, uninstall);
    await page.waitForTimeout(1200);
  }

  // 5) 检查更新
  cue(
    "点「检查更新」可查插件是否有新版；有新版时条目上出现红色徽标与「更新」按钮",
    "\"Check for updates\" looks for newer versions; when one exists, a red badge and an \"Update\" button appear on the row",
    "「更新を確認」で新しいバージョンがあるか確認できます。ある場合は赤いバッジと「更新」ボタンが行に表示されます",
  );
  const checkBtn = page.getByRole("button", { name: /检查更新/ }).first();
  if (await checkBtn.count()) {
    await clickAt(page, checkBtn);
    await page.waitForTimeout(1600);
  }

  saveCues("13-plugin-installed");
  await page.waitForTimeout(HOLD_END);
});
