import { test } from "@playwright/test";
import { mockTauri, makeVersion, makeInstance } from "../helpers.js";
import { installCursor, glide, clickAt, hover, scrollTo, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";
import { T } from "./i18n.js";

// 09 实例浮层：主按钮分体 + 共享 home / 实例列表 / 目标切换 / 行内动作。
// 旧版「已隔离 ▾」五项菜单已由 InstanceMenu 取代（v0.4.0）。
test("09 实例浮层", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.4.0",
      root_dir: "D:/dsh-data",
      default_version: "0.2.1-rc.2",
      isolated_versions: ["0.2.0-rc.1"],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [
      makeVersion("0.2.1-rc.2", { isDefault: true, installed_at: "2026-10-01" }),
      makeVersion("0.2.0-rc.1", { isolated: true, installed_at: "2026-09-28" }),
    ],
    instances: [
      makeInstance("i1", "默认隔离", { mounted_versions: ["0.2.0-rc.1"] }),
      makeInstance("i2", "对比用", { mounted_versions: ["0.2.0-rc.1"] }),
    ],
    versionTargets: { "0.2.0-rc.1": "i1" },
    runningTargets: [{ kind: "instance", version: "0.2.0-rc.1", instance: "i1" }],
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  await scrollTo(page, ".ver-list", -120);
  await page.waitForTimeout(400);

  cue(
    "主按钮改为分体式：左边运行当前目标，右边小三角打开实例浮层",
    "The run button is now split: the left side runs the selected target, the caret opens the instance menu",
    "実行ボタンは分割式になりました。左側は選択中のターゲットを実行し、右側の三角でインスタンスメニューを開きます",
  );
  await hover(page, page.locator(".run-caret").nth(1));
  await page.waitForTimeout(500);
  await clickAt(page, page.locator(".run-caret").nth(1));
  await page.locator(".instance-menu").waitFor({ state: "visible", timeout: 5000 });
  await page.waitForTimeout(900);

  cue(
    "浮层顶部固定共享 home 行；下面是该版本挂载的实例，单选框决定主按钮目标",
    "The shared-home row is pinned at the top; below are the instances mounted to this version. The radio sets the main-button target",
    "上部に共有 home 行、その下にこのバージョンにマウントされたインスタンスが並び、ラジオでメインボタンのターゲットを選びます",
  );
  const sharedRow = page.locator(".im-row.shared");
  const rb = await sharedRow.boundingBox();
  if (rb) {
    await glide(page, rb.x + rb.width / 2, rb.y + rb.height / 2);
    await page.waitForTimeout(700);
  }

  cue(
    "实例行显示运行状态；运行中的实例按钮变为「聚焦」，删除禁用",
    "Instance rows show running state; a running instance shows \"Focus\" and its delete button is disabled",
    "インスタンス行には実行状態が表示されます。実行中の行は「フォーカス」になり、削除は無効になります",
  );
  const runningRow = page.locator(".im-row", { hasText: "默认隔离" });
  const rr = await runningRow.boundingBox();
  if (rr) {
    await glide(page, rr.x + rr.width / 2, rr.y + rr.height / 2);
    await page.waitForTimeout(900);
  }
  const disabledDelete = runningRow.locator(".im-act.danger");
  if (await disabledDelete.count()) {
    const db = await disabledDelete.boundingBox();
    if (db) {
      await glide(page, db.x + db.width / 2, db.y + db.height / 2);
      await page.waitForTimeout(700);
    }
  }

  cue(
    "切到「对比用」：主按钮立即变为「运行 对比用」，下次点运行就使用该实例的 home",
    "Switch to \"Comparison\": the main button becomes \"Run Comparison\", so the next run uses that instance's home",
    "「比較用」に切り替えると、メインボタンが「比較用を実行」に変わり、次回の実行はそのインスタンスの home を使います",
  );
  const targetRadio = page.locator(".im-row", { hasText: "对比用" }).locator('input[type="radio"]');
  await clickAt(page, targetRadio);
  await page.waitForTimeout(900);
  const mainBtn = page.getByRole("button", { name: T("instances.runInstance", { name: "对比用" }) }).first();
  if (await mainBtn.count()) await hover(page, mainBtn);
  await page.waitForTimeout(800);

  cue(
    "每行可打开目录、改名、删除；底部「＋ 新建实例」创建只挂当前版本的新实例",
    "Each row can open its folder, rename or delete; \"+ New instance\" at the bottom creates one mounted to this version",
    "各行からフォルダを開く・名前変更・削除ができます。下部の「＋ 新規インスタンス」はこのバージョンにマウントした新規作成です",
  );
  const acts = page.locator(".im-act");
  const n = await acts.count();
  for (let i = 0; i < Math.min(n, 6); i++) {
    const b = await acts.nth(i).boundingBox();
    if (b) {
      await glide(page, b.x + b.width / 2, b.y + b.height / 2);
      await page.waitForTimeout(450);
    }
  }
  const create = page.locator(".im-create");
  if (await create.count()) await hover(page, create);
  await page.waitForTimeout(900);

  saveCues("09-isolated");
  await page.waitForTimeout(HOLD_END);
});
