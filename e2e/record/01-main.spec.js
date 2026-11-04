import { test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, scrollTo, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 01 快速上手四步：对应正文 73-78 行。
//   1. 看环境检查
//   2. 点「刷新列表」从 npm 拉取可安装版本
//   3. 点版本号填入输入框 → 点「安装」→ 进度条走完
//   4. 点「运行」
const NEW_VER = "0.2.1-rc.2";

// dsh 界面截图（fixture）以 base64 data URL 注入，供第 4 步展示内嵌窗口内容。
// 用 data URL 而非文件路径：page.evaluate 里拿不到 Node 的 fs。
const DSH_UI_SRC =
  "data:image/png;base64," + readFileSync(join("e2e", "record", "fixtures", "dsh-ui.png")).toString("base64");

/** 模拟一次安装的 8 阶段进度事件流（字段见 versions/install.rs 的 ProgressEvent）。 */
async function playInstall(page) {
  const steps = [
    { step: 1, stage: "准备目录", detail: "", fraction: 1 },
    { step: 2, stage: "写入配置", detail: "", fraction: 1 },
    { step: 3, stage: "启动 pnpm", detail: "", fraction: 1 },
    { step: 4, stage: "解析依赖", detail: "已解析 312 个包", fraction: 1 },
    { step: 5, stage: "下载依赖", detail: "下载 1.2 GB", fraction: 0.35 },
    { step: 5, stage: "下载依赖", detail: "下载 2.6 GB", fraction: 0.72 },
    { step: 5, stage: "下载依赖", detail: "下载 3.6 GB", fraction: 1 },
    { step: 6, stage: "写入文件", detail: "写入 4821 / 12045", fraction: 0.4 },
    { step: 6, stage: "写入文件", detail: "写入 9640 / 12045", fraction: 0.8 },
    { step: 6, stage: "写入文件", detail: "写入 12045 / 12045", fraction: 1 },
    { step: 7, stage: "构建原生模块", detail: "", fraction: 0.6 },
    { step: 7, stage: "构建原生模块", detail: "", fraction: 1 },
    { step: 8, stage: "完成", detail: "", fraction: 1 },
  ];
  for (const s of steps) {
    await page.evaluate((p) => window.__mockEmit("install-progress", p), {
      step: s.step, total: 8, stage: s.stage, detail: s.detail, fraction: s.fraction,
    });
    await page.waitForTimeout(s.step >= 5 && s.step <= 6 ? 280 : 160);
  }
}

test("01 快速上手四步", async ({ page }) => {
  startCues();
  await installCursor(page);

  // 首次打开：没有任何已安装版本
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
    installed: [],
    extra: {
      // 安装挂起 4.2 秒，进度由 playInstall 手动推
      install_version: "(args) => new Promise((r) => setTimeout(() => r('已安装 ' + (args?.version || '')), 4200))",
      // 运行挂起 2.5 秒，让启动态（splash）能被录到
      run_version: "(args) => new Promise((r) => setTimeout(() => r('已在窗口打开 DSH ' + (args?.version || '')), 2500))",
      is_version_open: "() => Promise.resolve(false)",
      // 第 1 次 refresh（首屏）返回空；之后返回已安装该版本
      list_installed: `() => {
        window.__installedCalls = (window.__installedCalls || 0) + 1;
        if (window.__installedCalls < 2) return Promise.resolve([]);
        return Promise.resolve([{ version: '${NEW_VER}', installed_at: '2026-10-01', isolated: true, is_default: false, broken: false, note: null }]);
      }`,
    },
  });
  // 暴露 dsh 界面截图给页面内脚本用
  await page.addInitScript((src) => { window.__DSH_UI_SRC = src; }, DSH_UI_SRC);
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // ── 第 1 步：看环境检查（正文 75 行）──
  cue(
    "第 1 步：看环境检查 —— Node / pnpm / 磁盘 / npm 源，致命项必须先装好",
    "Step 1: Check the environment — Node, pnpm, disk and npm registry; critical items must pass first",
    "ステップ 1：環境チェック —— Node / pnpm / ディスク / npm レジストリ。必須項目は先に整える",
  );
  await glide(page, 300, 100);
  await page.waitForTimeout(500);
  await glide(page, 300, 270);
  await page.waitForTimeout(800);

  // ── 第 2 步：点「刷新列表」拉取版本（正文 76 行）──
  cue(
    "第 2 步：点「刷新列表」，从 npm 拉取所有可安装的 dsh 版本",
    "Step 2: Click \"Refresh list\" to fetch all installable dsh versions from npm",
    "ステップ 2：「リスト更新」をクリックし、npm からインストール可能な dsh バージョンを取得",
  );
  await clickAt(page, page.getByRole("button", { name: /刷新列表/ }).first());
  await page.locator(".chip").first().waitFor({ state: "visible", timeout: 8000 });
  await page.waitForTimeout(900);

  // ── 第 3 步：点版本号 → 填入输入框 → 关掉更新说明 → 点「安装」→ 进度条走完（正文 77 行）──
  cue(
    "第 3 步：点版本号填入输入框，再点「安装」，观察进度条走完",
    "Step 3: Click a version to fill the input, then click \"Install\" and watch the progress bar",
    "ステップ 3：バージョン番号をクリックして入力欄に反映し、「インストール」で進捗バーを確認",
  );
  // 指定版本，与 list_installed mock 返回的一致
  await clickAt(page, page.locator(".chip", { hasText: NEW_VER }).first());
  await page.waitForTimeout(700);

  // 更新说明面板会占位（头部整行可点关闭），点它收起，露出安装进度区
  const noteHead = page.locator(".notes-head").first();
  if (await noteHead.count()) {
    await clickAt(page, noteHead);
    await page.waitForTimeout(400);
  } else {
    // 兜底：再点一次 chip 收起说明
    await clickAt(page, page.locator(".chip").first());
    await page.waitForTimeout(400);
  }

  await clickAt(page, page.getByRole("button", { name: "安装", exact: true }).first());
  await page.waitForTimeout(500);
  await playInstall(page);
  await page.waitForTimeout(1200);

  // ── 第 4 步：点「运行」（正文 78 行）──
  cue(
    "第 4 步：点「运行」，在应用内嵌窗口中打开该版本的 dsh Web 界面",
    "Step 4: Click \"Run\" to open this version's dsh web UI in an embedded window",
    "ステップ 4：「実行」をクリックし、アプリ内の埋め込みウィンドウで dsh の Web UI を開く",
  );
  // 滚到已安装列表，确保按钮在可视区
  await scrollTo(page, ".ver-list", -120);
  await page.waitForTimeout(500);
  const runBtn = page.getByRole("button", { name: "运行", exact: true }).first();
  if (await runBtn.count()) {
    await clickAt(page, runBtn);
    // 等启动态出现（内嵌窗口打开中）
    await page.locator(".splash").waitFor({ state: "visible", timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(1500);

    // 启动完成后：模拟内嵌窗口弹出，展示真实的 dsh Web 界面
    // （Tauri 里是独立 OS 窗口；浏览器 mock 环境下用覆盖层模拟）
    await page.evaluate(async () => {
      const src = window.__DSH_UI_SRC;
      if (!src) return;
      const mask = document.createElement("div");
      mask.id = "__dsh_embed_mask";
      mask.style.cssText =
        "position:fixed;inset:0;z-index:2147483640;background:rgba(18,20,24,.55);" +
        "display:flex;align-items:center;justify-content:center";
      const win = document.createElement("div");
      win.style.cssText =
        "position:relative;height:calc(100% - 64px);aspect-ratio:1100 / 760;" +
        "max-width:calc(100% - 64px);" +
        "border-radius:10px;overflow:hidden;box-shadow:0 18px 48px rgba(0,0,0,.45);" +
        "background:#fff";
      const img = document.createElement("img");
      img.src = src;
      img.style.cssText = "width:100%;height:100%;object-fit:contain;display:block;background:#f5f6f8";
      win.appendChild(img);
      mask.appendChild(win);
      document.body.appendChild(mask);
    });
    await page.waitForTimeout(2600);

    // 第 5 屏：dsh 界面已就绪（正文 78 行"在应用内嵌窗口中打开该版本的 dsh Web 界面"）
    cue(
      "dsh 的 Web 界面在独立的内嵌窗口中打开；关闭窗口即停止该版本的 dsh 进程",
      "The dsh web UI opens in a separate embedded window; closing the window stops that version's dsh process",
      "dsh の Web UI は独立した埋め込みウィンドウで開きます。ウィンドウを閉じると、そのバージョンの dsh プロセスも停止します",
    );
    await page.waitForTimeout(1400);
  }

  saveCues("01-main");
  await page.waitForTimeout(HOLD_END);
});
