import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, clickAt, hover, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";
import { T, REC_LANG } from "./i18n.js";

const DESC = {
  "zh-CN": "一个显示在 dsh 侧栏的小鲸鱼宠物。",
  "zh-TW": "顯示在 dsh 側欄的小鯨魚寵物。",
  "en-US": "A tiny whale pet for the dsh sidebar.",
  "ja-JP": "dsh のサイドバーに表示される小さなクジラのペット。",
  "ko-KR": "dsh 사이드바에 표시되는 작은 고래 펫.",
};

// 18 添加插件对话框：form → checking → confirm → installing → done。
// 对应正文「dsh 插件 · 添加插件」。
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
  ],
};

test("18 添加插件", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.4.0",
      root_dir: "D:/dsh-data",
      default_version: null,
      isolated_versions: [],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [makeVersion("0.2.1-rc.2")],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_profile_info: {
        runtime_version: "0.2.1",
        exemptions: {},
        pending_builds: [],
        logs_dir: "D:/dsh/logs",
      },
      ds_probe_registries: { kind: "official", url: "https://registry.npmjs.org", ms: 42 },
      ds_inspect_spec: {
        kind: "npm",
        name: "dsh-plugin-whale-pet",
        version: "1.0.0",
        description: DESC[REC_LANG] || DESC["zh-CN"],
        declares_bundle: true,
        problem: null,
        detail: null,
      },
      ds_plugin_action: "(args) => new Promise((r) => setTimeout(() => r({ name: 'dsh-plugin-whale-pet' }), 2200))",
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  cue(
    "在插件面板点「添加插件」，打开官方风格的安装对话框",
    "Click Add plugin in the plugin panel to open the official-style install dialog",
    "プラグインパネルの「プラグインを追加」から、公式風のインストールダイアログを開きます",
  );
  const pluginBtn = page.getByRole("button", { name: T("dsplugins.button"), exact: true }).first();
  await clickAt(page, pluginBtn);
  await page.locator(".dp-head").waitFor({ state: "visible", timeout: 8000 });
  await page.waitForTimeout(600);
  const addBtn = page.getByRole("button", { name: T("dsplugins.addPlugin") }).first();
  await clickAt(page, addBtn);
  await page.locator(".dp-modal").waitFor({ state: "visible", timeout: 5000 });
  await page.waitForTimeout(800);

  cue(
    "支持 npm 包名、GitHub 地址或本地目录；可先选安装源。首次使用会自动探测最快源",
    "Supports npm package names, GitHub URLs or local folders; pick a registry first. The fastest registry is probed on first use",
    "npm パッケージ名、GitHub URL、ローカルフォルダに対応。先にインストール元を選べます。初回は最速のレジストリを自動検出します",
  );
  const specInput = page.locator(".dp-modal-input").first();
  await clickAt(page, specInput);
  await page.keyboard.type("dsh-plugin-whale-pet", { delay: 65 });
  await page.waitForTimeout(600);
  const registry = page.locator(".dp-modal .dp-select").first();
  if (await registry.count()) {
    await registry.selectOption("official");
    await page.waitForTimeout(500);
  }
  await hover(page, page.locator(".dp-modal-install"));
  await page.waitForTimeout(700);

  cue(
    "点「安装」先做 inspect：确认包名、版本、描述与安装源，再显示信任提示",
    "Install first inspects the spec: package, version, description and registry, then shows the trust warning",
    "「インストール」を押すとまず inspect を実行し、パッケージ名・バージョン・説明・インストール元を確認してから信頼警告を表示します",
  );
  await clickAt(page, page.locator(".dp-modal-install"));
  await page.locator(".dp-confirm-row").first().waitFor({ state: "visible", timeout: 8000 });
  await page.waitForTimeout(1200);

  cue(
    "确认页显示插件名、版本、描述与安装源；确认后由官方 dsh CLI 执行安装",
    "The confirmation page shows name, version, description and registry; after confirming, the official dsh CLI installs it",
    "確認画面には名前・バージョン・説明・インストール元が表示されます。確認後は公式 dsh CLI がインストールします",
  );
  const confirm = page.locator(".dp-modal-actions .dp-btn-primary").first();
  if (await confirm.count()) {
    await hover(page, confirm);
    await page.waitForTimeout(700);
    await clickAt(page, confirm);
  }
  await page.waitForTimeout(1200);

  cue(
    "安装中显示实时输出，且当前版本不支持取消；完成后提示「安装成功不保证激活」",
    "Installing shows live output and cannot be cancelled in this version; the done page notes that installation does not guarantee activation",
    "インストール中はリアルタイム出力を表示し、このバージョンではキャンセルできません。完了後は「インストール成功＝有効化を保証しない」と表示します",
  );
  await page.waitForTimeout(2200);
  const done = page.locator(".dp-done-title");
  if (await done.count()) await hover(page, done.first());
  await page.waitForTimeout(1000);

  saveCues("18-plugin-add");
  await page.waitForTimeout(HOLD_END);
});
