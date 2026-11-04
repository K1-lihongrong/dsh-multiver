import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 07 网络失败换源：安装抛网络错误 → 弹出换源对话框 → 切换源下拉。
test("07 换源弹窗", async ({ page }) => {
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
      // 网络类错误：classifyError 命中 "ERR_PNPM_FETCH" → network
      install_version: '(args) => Promise.reject("ERR_PNPM_FETCH_ECONNRESET: request to https://registry.npmjs.org/@deepseek-ai%2fdsh failed, reason: read ECONNRESET")',
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  cue(
    "安装时网络请求失败",
    "The install request fails due to a network error",
    "インストール時のネットワークリクエストが失敗",
  );
  const input = page.locator(".install-row input").first();
  const box = await input.boundingBox();
  if (box) {
    await glide(page, box.x + 60, box.y + box.height / 2);
    await page.mouse.click(box.x + 60, box.y + box.height / 2);
    await page.waitForTimeout(150);
    await page.keyboard.type("0.2.1-rc.5", { delay: 70 });
    await page.waitForTimeout(300);
  }
  await clickAt(page, page.getByRole("button", { name: "安装", exact: true }).first());
  // 等换源弹窗真正出现（安装失败 → 分类 → 弹框，链路有几次 await）
  await page.locator(".modal-mask .modal").first().waitFor({ state: "visible", timeout: 10000 });
  await page.waitForTimeout(900);

  // 正文 199 行："弹出换源对话框（官方 / 阿里云 / 腾讯云 / 华为云 / 自定义），一键重试"
  // 依次切换几个源，最后悬停「换源重试」按钮
  cue(
    "弹出换源对话框：可选官方 / 阿里云 / 腾讯云 / 华为云，或填自定义源，一键重试",
    "A registry dialog appears: choose official, Aliyun, Tencent, Huawei, or enter a custom registry, then retry with one click",
    "レジストリ切り替えダイアログが表示されます：公式 / Aliyun / Tencent / Huawei、またはカスタムレジストリを指定してワンクリックで再試行",
  );
  const select = page.locator(".modal select").first();
  if (await select.count()) {
    const sb = await select.boundingBox();
    if (sb) {
      await glide(page, sb.x + sb.width / 2, sb.y + sb.height / 2);
      await page.waitForTimeout(300);
      // 官方 → 阿里云 → 腾讯云（逐个展示源名称）
      for (const idx of [1, 2, 3]) {
        await select.selectOption({ index: idx });
        await page.waitForTimeout(700);
      }
      // 回到阿里云（国内最常用）
      await select.selectOption({ index: 1 });
      await page.waitForTimeout(600);
    }
  }

  // 展开「原始错误」，让观众看到失败原因
  const rawErr = page.locator(".modal details, .modal summary").first();
  if (await rawErr.count()) {
    await clickAt(page, rawErr);
    await page.waitForTimeout(800);
  }

  // 悬停「换源重试」主按钮（正文 199 行的"一键重试"）
  const retryBtn = page.locator(".modal .btn.primary").first();
  if (await retryBtn.count()) {
    const rb = await retryBtn.boundingBox();
    if (rb) {
      await glide(page, rb.x + rb.width / 2, rb.y + rb.height / 2);
      await page.waitForTimeout(800);
    }
  }
  saveCues("07-modal-net");
  await page.waitForTimeout(HOLD_END);
});
