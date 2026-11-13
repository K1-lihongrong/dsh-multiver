import { test } from "@playwright/test";
import { mockTauri, makeVersion, makeInstance } from "../helpers.js";
import { installCursor, glide, clickAt, hover, waitReady, startCues, cue, saveCues, HOLD_START, HOLD_END } from "./_rec.js";
import { T } from "./i18n.js";

// 17 全局实例面板：孤儿置顶、未挂载角标、扫描占用、新建、改名、运行中禁删。
const V = "0.2.1-rc.2";
const V2 = "0.2.0-rc.1";

test("17 实例管理面板", async ({ page }) => {
  startCues();
  await installCursor(page);
  await mockTauri(page, {
    state: {
      version: "0.4.0",
      root_dir: "D:/dsh-data",
      default_version: V,
      isolated_versions: [V2],
      broken_versions: [],
      use_official_dsh_home: false,
      runtime_prompt_dismissed: true,
      maintenance: { auto_enabled: true, last_cleanup_at: null, last_cleanup_count: 0, last_prune_at: null },
    },
    installed: [
      makeVersion(V, { isDefault: true }),
      makeVersion(V2, { isolated: true }),
    ],
    instances: [
      makeInstance("i1", "默认隔离", { mounted_versions: [V2] }),
      makeInstance("i2", "对比用", { mounted_versions: [V, V2], running_on: V2 }),
      makeInstance("i9", "旧版遗留", { mounted_versions: [] }),
    ],
    versionTargets: { [V]: "i2", [V2]: "i1" },
    runningTargets: [{ kind: "instance", version: V2, instance: "i2" }],
    extra: {
      scan_instance_size: "() => Promise.resolve(734003200)",
      instance_create: `(args) => {
        const id = 'i10';
        const home = 'D:/dsh-data/homes/' + id;
        window.__mockInstances.push({
          id, name: args.name, created_at: 1762300000, state: 'ready', home,
          mounted_versions: args.attach || [], orphan: !(args.attach && args.attach.length),
          running_on: null, size: null,
        });
        for (const v of (args.attach || [])) window.__mockVersionTargets[v] = id;
        return id;
      }`,
      instance_rename: `(args) => {
        const inst = window.__mockInstances.find((x) => x.id === args.id);
        if (inst) inst.name = args.name;
        return null;
      }`,
    },
  });
  await page.goto("/");
  await waitReady(page);
  await page.waitForTimeout(HOLD_START);

  cue(
    "版本列表上方的「实例管理」按钮：孤儿数量以角标显示，打开全局实例面板",
    "The Instances button above the version list shows an orphan badge and opens the global instance panel",
    "バージョン一覧上部の「インスタンス管理」ボタンには未マウント件数のバッジが付き、グローバルパネルを開きます",
  );
  await hover(page, page.locator(".instance-panel-btn"));
  await page.waitForTimeout(500);
  await clickAt(page, page.locator(".instance-panel-btn"));
  await page.locator(".ip-modal").waitFor({ state: "visible", timeout: 5000 });
  await page.waitForTimeout(1000);

  cue(
    "面板列出全部实例：孤儿（未挂载）置顶并标黄，每行有 home、挂载版本、主按钮目标与运行状态",
    "The panel lists every instance: orphans (unmounted) are pinned and highlighted, with home, mounted versions, main-button target and running state",
    "パネルには全インスタンスが並び、未マウント（孤児）は先頭に黄色で表示されます。home、マウント済みバージョン、メインボタン、実行状態も確認できます",
  );
  const orphan = page.locator(".ip-row.orphan");
  const ob = await orphan.boundingBox();
  if (ob) {
    await glide(page, ob.x + ob.width / 2, ob.y + ob.height / 2);
    await page.waitForTimeout(900);
  }

  cue(
    "点「扫描占用」计算该实例 home 的大小；运行中的实例删除按钮禁用",
    "Click Scan usage to measure the instance home; the delete button is disabled while it is running",
    "「使用量をスキャン」でインスタンス home のサイズを計算します。実行中のインスタンスは削除ボタンが無効です",
  );
  const i1Row = page.locator(".ip-row", { hasText: "默认隔离" });
  const scan = i1Row.locator(".ip-link");
  if (await scan.count()) {
    await clickAt(page, scan);
    await page.waitForTimeout(900);
  }
  const runningRow = page.locator(".ip-row", { hasText: "对比用" });
  const del = runningRow.locator(".ip-delete");
  if (await del.count()) {
    const db = await del.boundingBox();
    if (db) {
      await glide(page, db.x + db.width / 2, db.y + db.height / 2);
      await page.waitForTimeout(800);
    }
  }

  cue(
    "点「＋ 新建实例」填写名称并勾选挂载版本；创建后列表立即出现新行",
    "Click + New instance, enter a name and tick the versions to mount; the new row appears immediately",
    "「＋ 新規インスタンス」で名前を入力し、マウントするバージョンにチェックします。作成すると一覧にすぐ表示されます",
  );
  const newBtn = page.getByRole("button", { name: T("instances.new") }).first();
  await clickAt(page, newBtn);
  await page.waitForTimeout(400);
  const nameInput = page.locator(".ip-create-form .ip-input").first();
  await clickAt(page, nameInput);
  await page.keyboard.type("演示实例", { delay: 80 });
  await page.locator(".ip-versions input[type=checkbox]").first().check();
  await page.waitForTimeout(300);
  await clickAt(page, page.locator(".ip-create-form").getByRole("button", { name: T("instances.create"), exact: true }));
  await page.waitForTimeout(1200);

  cue(
    "新实例出现在列表中；可继续改名、挂载/解除挂载，或删除（home 移入回收站）",
    "The new instance appears in the list; you can rename it, mount/unmount versions, or delete it (home moves to trash)",
    "新しいインスタンスが一覧に表示されます。名前変更、マウント/解除、削除（home はゴミ箱へ）が可能です",
  );
  const newRow = page.locator(".ip-row", { hasText: "演示实例" });
  if (await newRow.count()) {
    const nb = await newRow.boundingBox();
    if (nb) {
      await glide(page, nb.x + nb.width / 2, nb.y + nb.height / 2);
      await page.waitForTimeout(600);
    }
    await clickAt(page, newRow.getByRole("button", { name: T("instances.rename"), exact: true }));
    await page.waitForTimeout(300);
    const renameInput = newRow.locator(".ip-input").first();
    await clickAt(page, renameInput);
    await page.keyboard.press("Control+A");
    await page.keyboard.type("演示实例 2", { delay: 70 });
    await clickAt(page, newRow.getByRole("button", { name: T("instances.save"), exact: true }));
    await page.waitForTimeout(900);
  }

  saveCues("17-instance-panel");
  await page.waitForTimeout(HOLD_END);
});
