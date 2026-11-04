import { test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { installCursor, glide, clickAt, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";

// 10 内嵌窗口顶栏圆点三态：dot → pill（悬停显示版本号）→ 面板（点击展开）。
//
// topbar.js 是后端用 initialization_script 注入到内嵌窗口的纯 DOM 脚本，
// 不依赖 dsh 后端；这里直接把它注入到一个空白页面，模拟三种状态。
test("10 顶栏圆点三态", async ({ page }) => {
  startCues();
  await installCursor(page);

  // 底图用真实的 dsh Web 界面截图，避免手写占位页与产品脱节。
  // 截图 1100x760（比例 1.447），视口 1280x800（比例 1.6）。
  // 图片按宽度撑满 1280 → 高 884，超出 800 共 84px；顶部对齐（top:0），
  // 超出的 84px 全部裁在底部（该区域是界面留白，无内容损失）。
  const dshUi =
    "data:image/png;base64," +
    readFileSync(join("e2e", "record", "fixtures", "dsh-ui.png")).toString("base64");
  await page.setContent(`<!DOCTYPE html><html><head><meta charset="utf-8"><style>
    html, body { margin:0; height:100%; overflow:hidden; background:#fff; }
    .dsh-shot {
      display:block;
      width:1280px; height:auto;
      position:absolute; left:0; top:0;
    }
  </style></head><body>
    <img class="dsh-shot" src="${dshUi}" alt="dsh Web 界面" />
  </body></html>`);

  // 注入 topbar.js（替换占位符）
  const src = readFileSync(join(process.cwd(), "src-tauri", "src", "topbar.js"), "utf8")
    .replace("__VER__", "0.2.1-rc.2")
    .replace("__URL__", "http://127.0.0.1:3080/?token=***");
  await page.addScriptTag({ content: src });
  await page.waitForTimeout(HOLD_START);

  // 1) dot 态：8px 小圆点，静置 1 秒
  cue(
    "内嵌窗口左上角有一个小圆点，平时是半透明的 8px 小圆点",
    "A small dot sits in the top-left corner of the embedded window; at rest it is a translucent 8px circle",
    "埋め込みウィンドウの左上に小さなドットがあります。通常は半透明の 8px の円です",
  );
  await page.waitForTimeout(900);

  // 2) pill 态：悬停 → 展开成含版本号的小胶囊
  cue(
    "鼠标悬停时展开成胶囊形，显示当前 dsh 版本号",
    "Hovering expands it into a pill showing the current dsh version",
    "マウスを乗せるとカプセル状に広がり、現在の dsh バージョンが表示されます",
  );
  await glide(page, 23, 23);
  await page.waitForTimeout(1100);

  // 3) 面板态：点击 → 展开面板（标题 / URL 输入框 / 重启按钮）
  cue(
    "点击圆点展开面板：版本号、完整 URL（可选中复制）、重启按钮、关闭",
    "Clicking the dot opens a panel: version, full URL (selectable), restart button, and close",
    "ドットをクリックするとパネルが開きます：バージョン、完全な URL（選択コピー可）、再起動ボタン、閉じる",
  );
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
  saveCues("10-topbar-dot");
  await page.waitForTimeout(HOLD_END);
});
