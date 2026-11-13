import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, scrollTo, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";
import { REC_LANG } from "./i18n.js";

const DIR_DESC = {
  "zh-CN": { logs: "启动错误与维护日志", homes: "实例隔离 home", webview: "各版本内嵌窗口数据", trash: "卸载回收站", assets: "内部资源（快捷方式图标）" },
  "zh-TW": { logs: "啟動錯誤與維護日誌", homes: "執行個體隔離 home", webview: "各版本內嵌視窗資料", trash: "解除安裝回收站", assets: "內部資源（捷徑圖示）" },
  "en-US": { logs: "Startup and maintenance logs", homes: "Instance home directories", webview: "Embedded-window data per version", trash: "Uninstall trash", assets: "Internal assets (shortcut icons)" },
  "ja-JP": { logs: "起動エラーとメンテナンスログ", homes: "インスタンス専用 home", webview: "バージョン別埋め込みウィンドウデータ", trash: "アンインストールのゴミ箱", assets: "内部リソース（ショートカットアイコン）" },
  "ko-KR": { logs: "시작 오류 및 유지보수 로그", homes: "인스턴스 격리 home", webview: "버전별 내장 창 데이터", trash: "제거 휴지통", assets: "내부 리소스(바로가기 아이콘)" },
};
const D = DIR_DESC[REC_LANG] || DIR_DESC["zh-CN"];

// 15 外观与语言：对应正文「八.1、外观与语言」。
//   1. 滚到「路径设置」底部的「外观与语言」区
//   2. 主题切到暗色 → 再切回亮色（展示实时生效）
//   3. 语言切到 English → 日本語 → 繁体中文 → 简体中文（展示界面文字随之切换）
test("15 外观与语言", async ({ page }) => {
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
        { name: "logs", desc: D.logs, path: "D:/dsh-data/logs" },
        { name: "homes", desc: D.homes, path: "D:/dsh-data/homes" },
        { name: "webview", desc: D.webview, path: "D:/dsh-data/webview" },
        { name: "trash", desc: D.trash, path: "D:/dsh-data/trash" },
        { name: "assets", desc: D.assets, path: "D:/dsh-data/assets" },
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

  // ── 第 1 步：定位到「外观与语言」区 ──
  cue(
    "「路径设置」面板底部有「外观与语言」：主题与语言两项偏好都保存在本地",
    "The \"Path settings\" panel has \"Appearance & Language\" at the bottom; both preferences are saved locally",
    "「パス設定」パネルの下部に「外観と言語」があります。テーマと言語の設定はローカルに保存されます",
  );
  await scrollTo(page, "#path-settings", -60);
  await page.waitForTimeout(600);
  const themeSelect = page.locator(".field-row select").first();
  await themeSelect.scrollIntoViewIfNeeded();
  await page.waitForTimeout(700);

  // ── 第 2 步：主题切到暗色 ──
  cue(
    "主题可选「跟随系统 / 亮色 / 暗色」，切换后界面立刻生效，无需重启",
    "Theme offers System / Light / Dark; changes apply instantly without restart",
    "テーマは「システムに従う / ライト / ダーク」から選択でき、切り替えるとすぐ反映されます",
  );
  const tbox = await themeSelect.boundingBox();
  if (tbox) {
    await glide(page, tbox.x + tbox.width / 2, tbox.y + tbox.height / 2);
    await page.waitForTimeout(300);
  }
  await themeSelect.selectOption("dark");
  await page.waitForTimeout(1200);

  // 切回亮色，展示两种状态
  cue(
    "切回亮色：整个界面（含本面板）立即换成浅色",
    "Switch back to Light: the whole UI, including this panel, turns light immediately",
    "ライトに戻すと、このパネルを含む画面全体がすぐに明るい配色になります",
  );
  await themeSelect.selectOption("light");
  await page.waitForTimeout(1400);

  // ── 第 3 步：语言切换 ──
  cue(
    "语言支持 简体中文 / 繁體中文 / English / 日本語 / 한국어，切换后界面文字立即更新",
    "Language supports Simplified Chinese, Traditional Chinese, English, Japanese and Korean; UI text updates immediately",
    "言語は簡体字中国語・繁体字中国語・英語・日本語・韓国語に対応し、切り替えると UI の文言がすぐに変わります",
  );
  const langSelect = page.locator(".field-row select").nth(1);
  const lbox = await langSelect.boundingBox();
  if (lbox) {
    await glide(page, lbox.x + lbox.width / 2, lbox.y + lbox.height / 2);
    await page.waitForTimeout(300);
  }
  cue(
    "切到 English：面板标题、按钮、提示全部变为英文",
    "Switched to English: panel titles, buttons and hints all become English",
    "English に切り替えると、パネルの見出し・ボタン・ヒントがすべて英語になります",
  );
  await langSelect.selectOption("en-US");
  await page.waitForTimeout(1200);
  cue(
    "切到日本語：界面文字随语言立即切换",
    "Switched to Japanese: the UI text changes immediately",
    "日本語に切り替えると、UI の文言がすぐに変わります",
  );
  await langSelect.selectOption("ja-JP");
  await page.waitForTimeout(1200);
  cue(
    "切到 한국어：五语界面均可用",
    "Switched to Korean: all five UI languages are available",
    "韓国語に切り替えると、5 言語すべての UI が利用できます",
  );
  await langSelect.selectOption("ko-KR");
  await page.waitForTimeout(1200);
  cue(
    "切到繁體中文：按鈕與提示同步更新",
    "Switched to Traditional Chinese: buttons and hints update together",
    "繁体字中国語に切り替えると、ボタンとヒントも同時に更新されます",
  );
  await langSelect.selectOption("zh-TW");
  await page.waitForTimeout(1200);
  cue(
    "最后切回简体中文。偏好保存在本地，下次打开自动恢复，首屏渲染前就应用，不会闪烁",
    "Finally back to Simplified Chinese. Preferences are saved locally and restored before first paint, with no flicker",
    "最後に簡体字中国語へ戻します。設定はローカルに保存され、次回起動時は初回描画前に適用されるためちらつきません",
  );
  await langSelect.selectOption("zh-CN");
  await page.waitForTimeout(1200);

  saveCues("15-appearance");
  await page.waitForTimeout(HOLD_END);
});
