import { test } from "@playwright/test";
import { mockTauri } from "../helpers.js";
import { installCursor, glide, hover, waitReady, HOLD_START, HOLD_END } from "./_rec.js";

// 02 环境检查未通过：展示缺失项的红色行 + 下载链接按钮。
// Node 与 pnpm 都缺失 → 出现「自动准备运行时」主按钮，符合手册描述。
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
  await page.waitForTimeout(400);
  // 悬停「自动准备运行时」按钮，展示主按钮 hover 光晕
  const btn = page.getByRole("button", { name: /自动准备运行时|准备运行时/ }).first();
  if (await btn.count()) await hover(page, btn);
  await page.waitForTimeout(700);
  await page.waitForTimeout(HOLD_END);
});
