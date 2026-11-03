import { test } from "@playwright/test";
import { mockTauri, makeVersion } from "../helpers.js";
import { installCursor, glide, clickAt, scrollTo, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 11 维护面板：滚动到路径设置里的维护区，展示开关、上次执行时间与三个按钮。
test("11 维护面板", async ({ page }) => {
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
      maintenance: {
        auto_enabled: true,
        // fmtTs 期望 Unix 秒：2026-11-02 08:15 UTC / 2026-10-28 21:40 UTC
        last_cleanup_at: 1793607300,
        last_cleanup_count: 3,
        last_prune_at: 1793223600,
      },
    },
    installed: [makeVersion("0.2.1-rc.2", { installed_at: "2026-10-01" })],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  // 滚到维护区（面板底部）
  await scrollTo(page, ".maint", -120);
  await page.waitForTimeout(700);

  // 正文 648-650 行：自动维护开关 + 规则说明 + 上次执行状态 + 三个手动按钮
  // 1) 演示开关：关 → 开（正文 648 行"关掉则启动时不做上述两件事"）
  const toggle = page.locator(".maint .checkbox-row input[type=checkbox]").first();
  if (await toggle.count()) {
    await clickAt(page, toggle);
    await page.waitForTimeout(500);
    await clickAt(page, toggle);
    await page.waitForTimeout(500);
  }

  // 2) 依次悬停三个手动按钮（正文 650 行）
  for (const name of [/清理孤立缓存/, /回收依赖仓库/, /打开日志目录/]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(750);
      }
    }
  }
  await page.waitForTimeout(HOLD_END);
});
