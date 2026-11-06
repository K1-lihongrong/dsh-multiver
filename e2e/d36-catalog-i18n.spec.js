import { test, expect } from "@playwright/test";
import { mockTauri, makeVersion } from "./helpers.js";

// v0.3.6 零.3：推荐页分类标签多语言。
// 分类的 key 保持中文原值（参与筛选），label 按当前语言取 categoryI18n。
// 非 ASCII 一律用 \u 转义，避免文件编码层把字符转坏。

const CAT = { ver: "\u7248\u672c\u7ba1\u7406", rem: "\u8fdc\u7a0b\u8bbf\u95ee" };

const CATALOG = {
  plugins: [
    { name: "p1", description: "d1", category: CAT.ver, repo: "https://x/1" },
    { name: "p2", description: "d2", category: CAT.rem, repo: "https://x/2" },
  ],
  categoryI18n: {
    [CAT.ver]: {
      "zh-TW": "\u7248\u672c\u7ba1\u7406",
      "en-US": "Version management",
      "ja-JP": "\u30d0\u30fc\u30b8\u30e7\u30f3\u7ba1\u7406",
      "ko-KR": "\ubc84\uc804 \uad00\ub9ac",
    },
    [CAT.rem]: {
      "zh-TW": "\u9060\u7aef\u5b58\u53d6",
      "en-US": "Remote access",
      "ja-JP": "\u30ea\u30e2\u30fc\u30c8\u30a2\u30af\u30bb\u30b9",
      "ko-KR": "\uc6d0\uaca9 \uc811\uc18d",
    },
  },
};

const TAB_LABEL = {
  "zh-CN": "\u63a8\u8350",
  "zh-TW": "\u63a8\u85a6",
  "en-US": "Recommended",
  "ja-JP": "\u304a\u3059\u3059\u3081",
  "ko-KR": "\ucd94\ucc9c",
};

const KO_VER = "\ubc84\uc804 \uad00\ub9ac";
const KO_REM = "\uc6d0\uaca9 \uc811\uc18d";

// 「插件」按钮在各语言下文字不同，按 aria-expanded 属性定位
function pluginButton(page) {
  return page.locator("button[aria-expanded]");
}

async function openCatalog(page, lang, catalog = CATALOG) {
  await mockTauri(page, {
    installed: [makeVersion("0.2.0-rc.1", { isolated: true })],
    extra: {
      ds_list_plugins: {
        dsh_home: "D:/dsh",
        home_kind: "shared",
        profiles: [{ name: "web", path: "D:/dsh/profiles/web", plugins: [] }],
      },
      ds_plugin_catalog: catalog,
      ds_plugin_catalog_check: { checked: true, updated: false, error: null },
    },
  });
  await page.addInitScript((l) => {
    try { localStorage.setItem("dsh-multiver.lang", l); } catch (_) {}
  }, lang);
  await page.goto("/");
  await pluginButton(page).click();
  // 推荐页是 tab 角色，不是 button
  await page.getByRole("tab", { name: TAB_LABEL[lang] }).click();
}

test("\u97e9\u8bed\u754c\u9762\u4e0b\u5206\u7c7b\u6807\u7b7e\u663e\u793a\u97e9\u8bed", async ({ page }) => {
  await openCatalog(page, "ko-KR");
  await expect(page.getByRole("button", { name: KO_VER })).toBeVisible();
  await expect(page.getByRole("button", { name: KO_REM })).toBeVisible();
  await expect(page.getByRole("button", { name: CAT.ver })).toHaveCount(0);
});

test("\u82f1\u6587\u754c\u9762\u4e0b\u5206\u7c7b\u6807\u7b7e\u663e\u793a\u82f1\u6587", async ({ page }) => {
  await openCatalog(page, "en-US");
  await expect(page.getByRole("button", { name: "Version management" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Remote access" })).toBeVisible();
});

test("\u7b5b\u9009\u4ecd\u6309\u4e2d\u6587 key \u5339\u914d", async ({ page }) => {
  await openCatalog(page, "ko-KR");
  await page.getByRole("button", { name: KO_REM }).click();
  await expect(page.locator(".dp-catalog-card")).toHaveCount(1);
  await expect(page.locator(".dp-catalog-card")).toContainText("p2");
});

test("\u7f3a categoryI18n \u65f6\u56de\u9000\u4e2d\u6587\u539f\u503c", async ({ page }) => {
  await openCatalog(page, "ko-KR", { plugins: CATALOG.plugins });
  await expect(page.getByRole("button", { name: CAT.ver })).toBeVisible();
});
