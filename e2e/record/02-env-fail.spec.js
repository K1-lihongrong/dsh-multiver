import { test } from "@playwright/test";
import { mockTauri } from "../helpers.js";
import { installCursor, glide, hover, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";
import { T } from "./i18n.js";

// 02 环境检查未通过：对应正文 86-102 行。
//   "启动时自动检查 6 项"
//   "缺失项会给出**一键下载链接**和安装步骤"
//
// 演示顺序：浏览检查列表 → 悬停失败项的「打开下载页」→
// 顶部横幅三个按钮 → 「自动准备运行时」→ 「去官网安装」→ 「重新检查」
test("02 环境检查未通过", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    extra: {
      check_env: [
        { name: "Node.js", name_key: "env.node.name", ok: false, detail: "", detail_key: "env.node.detail_missing", detail_args: {}, critical: true, install_hint: "", hint_key: "env.node.hint_missing", install_url: "https://nodejs.org/" },
        { name: "pnpm", name_key: "env.pnpm.name", ok: false, detail: "", detail_key: "env.pnpm.detail_missing", detail_args: {}, critical: true, install_hint: "", hint_key: "env.pnpm.hint_missing", install_url: "https://pnpm.io/installation" },
        { name: "git", name_key: "env.git.name", ok: true, detail: "git version 2.47.3", detail_key: "", detail_args: {}, critical: false, install_hint: "", hint_key: "", install_url: "" },
        { name: "根目录可写", name_key: "env.writable.name", ok: true, detail: "D:/dsh-data", detail_key: "", detail_args: {}, critical: true, install_hint: "", hint_key: "", install_url: "" },
        { name: "磁盘空间", name_key: "env.disk.name", ok: true, detail: "可用 100 GB", detail_key: "", detail_args: {}, critical: false, install_hint: "", hint_key: "", install_url: "" },
        { name: "npm 源连通", name_key: "env.registry.name", ok: true, detail: "可达", detail_key: "", detail_args: {}, critical: false, install_hint: "", hint_key: "", install_url: "" },
      ],
      runtime_status: { system_node_ok: false, system_pnpm_ok: false, bundled_node_ok: false, bundled_pnpm_ok: false, portable_supported: true },
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 1) 滑过检查列表：先看两项致命失败
  cue(
    "环境检查：Node 与 pnpm 为致命项，缺失时标红并给出安装步骤",
    "Environment check: Node and pnpm are required; missing ones are marked red with setup steps",
    "環境チェック：Node と pnpm は必須項目。未導入の場合は赤く表示され、導入手順が示されます",
  );
  await glide(page, 300, 120);
  await page.waitForTimeout(500);
  await glide(page, 300, 200);
  await page.waitForTimeout(800);

  // 2) 一键下载链接
  cue(
    "每个缺失项都带「打开下载页」按钮，一键跳转官方安装页",
    "Each missing item has an \"Open download page\" button that jumps to the official installer",
    "不足している項目には「ダウンロードページを開く」ボタンがあり、公式サイトへ直接移動できます",
  );
  const dlBtns = page.getByRole("button", { name: T("env.openDownload") });
  if (await dlBtns.count()) {
    await hover(page, dlBtns.nth(0));
    await page.waitForTimeout(900);
    if (await dlBtns.count() > 1) {
      await hover(page, dlBtns.nth(1));
      await page.waitForTimeout(900);
    }
  }

  // 3) 继续下滑看完通过项
  cue(
    "共检查 6 项：致命项全部通过后，安装按钮才会放行",
    "Six checks in total: installation is allowed only after all critical items pass",
    "チェック項目は全部で 6 つ。必須項目がすべて合格するとインストールできます",
  );
  await glide(page, 300, 330);
  await page.waitForTimeout(700);

  // 4) 顶部运行时横幅
  cue(
    "顶部横幅提供便携运行时的三个选项：准备便携版 / 去官网安装 / 忽略",
    "The banner offers three options: prepare portable runtime, install from official site, or dismiss",
    "上部バナーには 3 つの選択肢があります：ポータブル版を準備 / 公式サイトからインストール / 無視",
  );
  for (const name of [T("runtime.prepare"), T("runtime.guide"), T("runtime.ignore")]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(700);
      }
    }
  }

  // 5) 自动准备运行时
  cue(
    "「自动准备运行时」一键下载便携版 Node 与 pnpm 到数据根目录，不写入系统",
    "\"Prepare runtime\" downloads portable Node and pnpm into the data root without touching the system",
    "「ランタイムを自動準備」は、ポータブル版の Node と pnpm をデータフォルダにダウンロードします（システムには影響しません）",
  );
  const prep = page.getByRole("button", { name: T("env.prepareRuntime") }).first();
  if (await prep.count()) {
    await hover(page, prep);
    await page.waitForTimeout(1000);
  }

  // 6) 去官网安装 + 重新检查
  cue(
    "也可以自行安装后点「重新检查」立即重新自检",
    "You can also install manually and click \"Recheck\" to re-run the check immediately",
    "手動でインストールした場合は「再チェック」をクリックすると、すぐに再検査できます",
  );
  for (const name of [T("env.installGuide"), T("env.recheck")]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(800);
      }
    }
  }

  saveCues("02-env-fail");
  await page.waitForTimeout(HOLD_END);
});
