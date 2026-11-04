import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 06 安装进度：点「安装」→ 逐条推 install-progress 事件 → 进度条按阶段推进。
// 事件字段来自 src-tauri/src/versions/install.rs 的 ProgressEvent：
// { step, total, stage, detail, fraction }
test("06 安装进度", async ({ page }) => {
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
    installed: [],
    extra: {
      // 让 install_version 挂起 6 秒，期间由测试脚本手动推事件
      install_version: "(args) => new Promise((r) => setTimeout(() => r('已安装 ' + args.version), 6500))",
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 填入版本号并点「安装」
  cue(
    "填入版本号，点「安装」",
    "Enter a version number and click \"Install\"",
    "バージョン番号を入力し、「インストール」をクリック",
  );
  const input = page.locator(".install-row input").first();
  const box = await input.boundingBox();
  if (box) {
    await glide(page, box.x + 60, box.y + box.height / 2);
    await page.mouse.click(box.x + 60, box.y + box.height / 2);
    await page.waitForTimeout(150);
    await page.keyboard.type("0.2.2-alpha.2", { delay: 70 });
    await page.waitForTimeout(300);
  }
  await clickAt(page, page.getByRole("button", { name: "安装", exact: true }).first());
  await page.waitForTimeout(300);

  // 逐条推进度事件，节奏与真实安装相近（8 个阶段）
  cue(
    "安装分 8 个阶段：准备目录 → 写入配置 → 启动 pnpm → 解析依赖 → 下载依赖 → 写入文件 → 构建原生模块 → 完成，进度条按阶段加权推进",
    "Installation runs in eight stages: prepare, config, launch pnpm, resolve, download, write, build native modules, done. The bar advances by weighted stages",
    "インストールは 8 段階：ディレクトリ準備 → 設定書き込み → pnpm 起動 → 依存解決 → ダウンロード → ファイル書き込み → ネイティブモジュールのビルド → 完了。進捗バーは段階ごとに進みます",
  );
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
    await page.evaluate((p) => window.__mockEmit("install-progress", { step: p.step, total: 8, stage: p.stage, detail: p.detail, fraction: p.fraction }), s);
    await page.waitForTimeout(s.step >= 5 && s.step <= 6 ? 450 : 260);
  }
  await page.waitForTimeout(900);
  saveCues("06-progress");
  await page.waitForTimeout(HOLD_END);
});
