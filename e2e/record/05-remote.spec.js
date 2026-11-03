import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 05 可用版本：刷新列表 → 搜索筛选 → 点击 chip 展开更新说明 → 再点收起。
test("05 可用版本列表", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: null,
      isolated_versions: [],
      // 标灰一个版本，展示"已知安装失败"样式
      broken_versions: ["0.1.5-rc.3"],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" })],
    extra: {
      // 更新说明：点击 chip 后展开
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

  // 点「刷新列表」拉取远端版本
  const refresh = page.getByRole("button", { name: /刷新列表/ }).first();
  await clickAt(page, refresh);
  await page.waitForTimeout(800);

  // 搜索框输入筛选词
  const search = page.locator(".remote-search").first();
  if (await search.count()) {
    const box = await search.boundingBox();
    if (box) {
      await glide(page, box.x + 40, box.y + box.height / 2);
      await page.mouse.click(box.x + 40, box.y + box.height / 2);
      await page.waitForTimeout(200);
      await page.keyboard.type("0.2", { delay: 90 });
      await page.waitForTimeout(600);
      // 清空筛选，回到完整列表
      await page.keyboard.press("Control+A");
      await page.keyboard.press("Backspace");
      await page.waitForTimeout(400);
    }
  }

  // 点一个版本 chip：填入输入框并展开更新说明
  const chip = page.locator(".chip").first();
  if (await chip.count()) {
    await clickAt(page, chip);
    await page.waitForTimeout(900);
  }
  await page.waitForTimeout(HOLD_END);
});
