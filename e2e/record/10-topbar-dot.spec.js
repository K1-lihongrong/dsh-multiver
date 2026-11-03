import { test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { installCursor, glide, clickAt, HOLD_START, HOLD_END } from "./_rec.js";

// 10 内嵌窗口顶栏圆点三态：dot → pill（悬停显示版本号）→ 面板（点击展开）。
//
// topbar.js 是后端用 initialization_script 注入到内嵌窗口的纯 DOM 脚本，
// 不依赖 dsh 后端；这里直接把它注入到一个空白页面，模拟三种状态。
test("10 顶栏圆点三态", async ({ page }) => {
  await installCursor(page);

  // 准备一个有内容的底图，避免纯白背景看不出圆点位置
  await page.setContent(`<!DOCTYPE html><html><head><meta charset="utf-8"><style>
    body { margin:0; font:14px/1.6 system-ui,sans-serif; background:#f6f7f9; color:#2b2f36; }
    .mock-dsh { padding:24px 32px; }
    .mock-dsh h1 { font-size:18px; margin:0 0 8px; }
    .mock-dsh p { margin:0 0 6px; color:#5a6270; font-size:13px; }
    .mock-card { margin-top:16px; background:#fff; border:1px solid #e3e6ea; border-radius:8px; padding:14px 18px; max-width:560px; }
  </style></head><body>
    <div class="mock-dsh">
      <h1>DeepSeek Harness</h1>
      <p>版本 0.2.1-rc.2 · 已连接</p>
      <div class="mock-card">
        <p>这是一个模拟的 dsh Web 界面。</p>
        <p>左上角的圆点是版本管理器注入的顶栏控件。</p>
      </div>
    </div>
  </body></html>`);

  // 注入 topbar.js（替换占位符）
  const src = readFileSync(join(process.cwd(), "src-tauri", "src", "topbar.js"), "utf8")
    .replace("__VER__", "0.2.1-rc.2")
    .replace("__URL__", "http://127.0.0.1:3080/?token=***");
  await page.addScriptTag({ content: src });
  await page.waitForTimeout(HOLD_START);

  // 1) dot 态：8px 小圆点，静置 1 秒
  await page.waitForTimeout(900);

  // 2) pill 态：悬停 → 展开成含版本号的小胶囊
  await glide(page, 23, 23);
  await page.waitForTimeout(1100);

  // 3) 面板态：点击 → 展开面板（标题 / URL 输入框 / 重启按钮）
  await page.mouse.down();
  await page.waitForTimeout(100);
  await page.mouse.up();
  await page.waitForTimeout(1400);

  // 悬停「重启」按钮，展示可操作态
  const restart = page.locator("#__dsh_panel button", { hasText: "重启" }).first();
  if (await restart.count()) {
    const rb = await restart.boundingBox();
    if (rb) {
      await glide(page, rb.x + rb.width / 2, rb.y + rb.height / 2);
      await page.waitForTimeout(800);
    }
  }
  await page.waitForTimeout(HOLD_END);
});
