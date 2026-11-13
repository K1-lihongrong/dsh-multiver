import { test } from "@playwright/test";
import { mockTauri } from "../helpers.js";
import { installCursor, glide, clickAt, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";
import { T } from "./i18n.js";

// 08 私有包下架：安装抛 404 + @deepseek-ai → private-package → 弹对应说明框。
test("08 私有包下架", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.3.4",
      root_dir: "D:/dsh-data",
      default_version: null,
      isolated_versions: [],
      // 该版本已被标记为失败
      broken_versions: ["0.0.1-rc.1"],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [],
    extra: {
      install_version: '(args) => Promise.reject("ERR_PNPM_FETCH_404  GET https://registry.npmjs.org/@deepseek-ai%2fdsh-internal: Not Found - 404")',
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  cue(
    "安装时依赖包返回 404",
    "A dependency returns 404 during install",
    "インストール時に依存パッケージが 404 を返却",
  );
  const input = page.locator(".install-row input").first();
  const box = await input.boundingBox();
  if (box) {
    await glide(page, box.x + 60, box.y + box.height / 2);
    await page.mouse.click(box.x + 60, box.y + box.height / 2);
    await page.waitForTimeout(150);
    await page.keyboard.type("0.0.1-rc.1", { delay: 70 });
    await page.waitForTimeout(300);
  }
  await clickAt(page, page.getByRole("button", { name: T("remote.install"), exact: true }).first());
  await page.locator(".modal-mask .modal").first().waitFor({ state: "visible", timeout: 10000 });
  await page.waitForTimeout(900);

  cue(
    "该版本官方发布不全（依赖已下架的私有包），任何源都装不了；版本会被标灰，仍可换源重试",
    "This version is incomplete upstream (it depends on removed private packages), so no registry can install it. The version is greyed out; you can still retry with another registry",
    "このバージョンは公式の公開が不完全で（削除されたプライベートパッケージに依存）、どのレジストリでもインストールできません。バージョンはグレー表示され、別レジストリで再試行は可能です",
  );
  // 悬停「重试」按钮展示主按钮 hover
  const retry = page.locator(".modal .btn.primary").first();
  if (await retry.count()) {
    const rb = await retry.boundingBox();
    if (rb) {
      await glide(page, rb.x + rb.width / 2, rb.y + rb.height / 2);
      await page.waitForTimeout(700);
    }
  }
  saveCues("08-modal-priv");
  await page.waitForTimeout(HOLD_END);
});
