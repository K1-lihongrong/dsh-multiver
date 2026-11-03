import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 07 网络失败换源：安装抛网络错误 → 弹出换源对话框 → 切换源下拉。
test("07 换源弹窗", async ({ page }) => {
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

  // 打开源下拉，展示可选镜像
  const select = page.locator(".modal select").first();
  if (await select.count()) {
    const sb = await select.boundingBox();
    if (sb) {
      await glide(page, sb.x + sb.width / 2, sb.y + sb.height / 2);
      await page.waitForTimeout(250);
      await page.mouse.click(sb.x + sb.width / 2, sb.y + sb.height / 2);
      await page.waitForTimeout(500);
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Enter");
      await page.waitForTimeout(700);
    }
  }
  await page.waitForTimeout(HOLD_END);
});
