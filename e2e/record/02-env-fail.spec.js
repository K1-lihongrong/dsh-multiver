import { test } from "@playwright/test";
import { mockTauri } from "../helpers.js";
import { installCursor, glide, hover, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 02 环境检查未通过：对应正文 86-102 行。
//   "启动时自动检查 6 项"
//   "缺失项会给出**一键下载链接**和安装步骤"
//
// 演示顺序：浏览检查列表 → 悬停失败项的「打开下载页」→
// 顶部横幅三个按钮 → 「自动准备运行时」→ 「去官网安装」→ 「重新检查」
test("02 环境检查未通过", async ({ page }) => {
  await installCursor(page);
  await mockTauri(page, {
    extra: {
      check_env: [
        { name: "Node.js", name_key: "env.node.name", ok: false, detail: "未安装", detail_key: "", detail_args: {}, critical: true, install_hint: "安装 Node.js 22.19+ 或 24.2+，安装后重新检查", hint_key: "", install_url: "https://nodejs.org/" },
        { name: "pnpm", name_key: "env.pnpm.name", ok: false, detail: "未安装", detail_key: "", detail_args: {}, critical: true, install_hint: "安装 pnpm 10+（建议 11+）", hint_key: "", install_url: "https://pnpm.io/installation" },
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

  // 1) 自上而下滑过检查列表：先看两项失败（正文 92-93 行的致命项）
  await glide(page, 300, 120);
  await page.waitForTimeout(500);
  await glide(page, 300, 200);
  await page.waitForTimeout(800);

  // 2) 正文 100 行"一键下载链接"：悬停 Node 的「打开下载页」
  const dlBtns = page.getByRole("button", { name: /打开下载页/ });
  if (await dlBtns.count()) {
    await hover(page, dlBtns.nth(0));
    await page.waitForTimeout(900);
    if (await dlBtns.count() > 1) {
      await hover(page, dlBtns.nth(1));
      await page.waitForTimeout(900);
    }
  }

  // 3) 继续下滑看完通过项（正文 88 行"自动检查 6 项"）
  await glide(page, 300, 330);
  await page.waitForTimeout(700);

  // 4) 顶部横幅三个按钮（运行时引导，v0.2.9+）
  for (const name of [/准备便携版/, /去官网安装/, /忽略/]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(700);
      }
    }
  }

  // 5) 「自动准备运行时」主按钮（正文 75 行的"先把致命项装好"）
  const prep = page.getByRole("button", { name: /自动准备运行时/ }).first();
  if (await prep.count()) {
    await hover(page, prep);
    await page.waitForTimeout(1000);
  }

  // 6) 「去官网安装」+「重新检查」（正文 75 行"安装后重新检查"）
  for (const name of [/^去官网安装$/, /重新检查/]) {
    const btn = page.getByRole("button", { name }).first();
    if (await btn.count()) {
      const bb = await btn.boundingBox();
      if (bb) {
        await glide(page, bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(800);
      }
    }
  }

  await page.waitForTimeout(HOLD_END);
});
