import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import {
  installCursor, glide, clickAt, hover, waitReady,
  startCues, cue, saveCues, HOLD_START, HOLD_END,
} from "./_rec.js";
import { T } from "./i18n.js";

// 14 插件面板 · 推荐标签：切换标签 → 分类筛选 → 悬停描述看全文 → 点安装
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

const CATALOG = {
  categoryI18n: {
    "工具增强": { "zh-CN": "工具增强", "zh-TW": "工具增強", "en-US": "Tooling", "ja-JP": "ツール強化", "ko-KR": "도구 강화" },
    "内容聚合": { "zh-CN": "内容聚合", "zh-TW": "內容聚合", "en-US": "Content", "ja-JP": "コンテンツ集約", "ko-KR": "콘텐츠 수집" },
  },
  plugins: [
    {
      name: "dsh-plugin-memory",
      description: "为 dsh 提供长期记忆能力，跨会话保留上下文要点，支持手动整理与自动摘要。",
      descriptionI18n: {
        "zh-TW": "為 dsh 提供長期記憶能力，跨工作階段保留上下文重點，支援手動整理與自動摘要。",
        "en-US": "Long-term memory for dsh: keeps key context across sessions, with manual editing and automatic summaries.",
        "ja-JP": "dsh に長期記憶を追加。セッションをまたいで文脈の要点を保持し、手動整理と自動要約に対応します。",
        "ko-KR": "dsh에 장기 기억을 제공합니다. 세션 간 컨텍스트 핵심을 보존하고 수동 정리와 자동 요약을 지원합니다.",
      },
      category: "工具增强",
      repo: "https://github.com/example/dsh-plugin-memory",
      starred: true,
    },
    {
      name: "dsh-plugin-websearch",
      description: "接入联网搜索，让 dsh 能读取最新网页内容并引用来源。",
      descriptionI18n: {
        "zh-TW": "接入聯網搜尋，讓 dsh 能讀取最新網頁內容並引用來源。",
        "en-US": "Web search for dsh: reads the latest pages and cites sources.",
        "ja-JP": "Web 検索を追加。最新のページ内容を読み取り、出典を引用できます。",
        "ko-KR": "웹 검색을 추가합니다. 최신 웹 페이지를 읽고 출처를 인용합니다.",
      },
      category: "工具增强",
      repo: "https://github.com/example/dsh-plugin-websearch",
    },
    {
      name: "dsh-plugin-rss",
      description: "订阅 RSS 与 Atom 源，定时聚合到你关注的工作区。",
      descriptionI18n: {
        "zh-TW": "訂閱 RSS 與 Atom 來源，定時彙整到你關注的工作區。",
        "en-US": "Subscribe to RSS and Atom feeds and aggregate them into your workspace.",
        "ja-JP": "RSS と Atom を購読し、定期的にワークスペースへ集約します。",
        "ko-KR": "RSS 및 Atom 피드를 구독하고 작업 공간으로 정기 집계합니다.",
      },
      category: "内容聚合",
      repo: "https://github.com/example/dsh-plugin-rss",
    },
    {
      name: "dsh-plugin-notes",
      description: "轻量笔记面板，随手记录并让 dsh 读取。",
      descriptionI18n: {
        "zh-TW": "輕量筆記面板，隨手記錄並讓 dsh 讀取。",
        "en-US": "A lightweight notes panel for quick capture that dsh can read.",
        "ja-JP": "軽量なノートパネル。思いついた内容を記録し、dsh から読めます。",
        "ko-KR": "가벼운 노트 패널. 빠르게 기록하고 dsh가 읽을 수 있습니다.",
      },
      category: "内容聚合",
      repo: "https://github.com/example/dsh-plugin-notes",
    },
  ],
};

test("14 插件面板 · 推荐", async ({ page }) => {
  startCues();
  await installCursor(page);

  await mockTauri(page, {
    installed: [makeVersion("0.2.1-rc.2", { isolated: true })],
    extra: {
      ds_list_plugins: { dsh_home: "D:/dsh", home_kind: "shared", profiles: [PROFILE] },
      ds_plugin_catalog: CATALOG,
    },
  });

  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 打开面板
  const pluginBtn = page.getByRole("button", { name: T("dsplugins.button"), exact: true }).first();
  await clickAt(page, pluginBtn);
  await page.locator(".dp-head").waitFor({ state: "visible", timeout: 8000 });
  await page.waitForTimeout(800);

  // 1) 切到推荐标签
  cue(
    "「推荐」标签提供一份插件清单，按分类筛选，点「安装」直接装到当前 profile",
    "The \"Recommended\" tab offers a curated plugin list, filterable by category; \"Install\" adds one straight to the current profile",
    "「おすすめ」タブにはプラグイン一覧があり、カテゴリで絞り込めます。「インストール」で現在のプロファイルに直接追加されます",
  );
  await clickAt(page, page.getByRole("tab", { name: T("dsplugins.tabCatalog") }).first());
  await page.locator(".dp-catalog-card").first().waitFor({ state: "visible", timeout: 8000 });
  await page.waitForTimeout(1400);

  // 2) 分类筛选
  cue(
    "顶部标签栏按分类筛选；分类多时右侧出现 ‹ › 滚动按钮，标签栏本身也可以按住拖动",
    "Category tabs filter the list; when there are many, ‹ › scroll buttons appear on the right, and the bar itself can be dragged",
    "上部のタブでカテゴリを絞り込めます。カテゴリが多いと右側に ‹ › のスクロールボタンが現れ、タブバー自体もドラッグできます",
  );
  const catTab = page.locator(".dp-cat-tab").nth(2);
  if (await catTab.count()) {
    await clickAt(page, catTab);
    await page.waitForTimeout(1300);
    // 切回全部
    const allTab = page.locator(".dp-cat-tab").first();
    await clickAt(page, allTab);
    await page.waitForTimeout(900);
  }

  // 3) 悬停描述（长描述渐隐 + 浮窗）
  cue(
    "卡片第一行是包名与「安装」，第二行是链接与描述；描述过长时右侧渐隐，悬停可看全文",
    "Each card shows the package name and \"Install\" on the first row, links and description on the second; long descriptions fade out and expand on hover",
    "カードは1行目にパッケージ名と「インストール」、2行目にリンクと説明。長い説明は右側がフェードし、ホバーで全文が表示されます",
  );
  const descWrap = page.locator(".dp-cc-descwrap").first();
  if (await descWrap.count()) {
    await hover(page, descWrap);
    await page.waitForTimeout(1500);
  }

  // 4) 点安装
  cue(
    "点「安装」即把该插件装到当前 profile，过程中按钮变为进度态",
    "Clicking \"Install\" adds the plugin to the current profile; the button switches to a progress state meanwhile",
    "「インストール」をクリックするとそのプラグインが現在のプロファイルに追加され、処理中はボタンが進行状態に変わります",
  );
  const installBtn = page.locator(".dp-cc-install").first();
  if (await installBtn.count()) {
    await clickAt(page, installBtn);
    await page.waitForTimeout(1600);
  }

  saveCues("14-plugin-catalog");
  await page.waitForTimeout(HOLD_END);
});
