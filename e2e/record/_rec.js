// 录屏专用工具：假光标 + 平稳操作 + 等待。
//
// 为什么需要假光标：Playwright 的 recordVideo 不录系统鼠标指针，
// 观众看不到"点了哪里"。这里注入一个跟随 page.mouse 的 div，
// 移动时显示为箭头、点击时收缩一圈，视频里可见。
//
// 用法：await installCursor(page); 之后所有 page.mouse.* 都会反映在画面里。

export async function installCursor(page) {
  await page.addInitScript(() => {
    const style = document.createElement("style");
    style.textContent = `
      #__rec_cursor {
        position: fixed; z-index: 2147483647; pointer-events: none;
        width: 18px; height: 18px; margin: -2px 0 0 -2px;
        transition: transform .08s ease-out, opacity .2s;
        opacity: 0;
        left: -100px; top: -100px;
      }
      #__rec_cursor.show { opacity: .9; }
      #__rec_cursor.click { transform: scale(.72); }
      #__rec_cursor::before {
        content: ""; position: absolute; left: 0; top: 0;
        width: 0; height: 0;
        border-left: 11px solid #111; border-top: 11px solid #111;
        border-right: 7px solid transparent; border-bottom: 7px solid transparent;
        transform: rotate(-8deg); transform-origin: 0 0;
        filter: drop-shadow(0 1px 2px rgba(0,0,0,.5));
      }
      #__rec_cursor::after {
        content: ""; position: absolute; left: 0; top: 0;
        width: 0; height: 0;
        border-left: 9px solid #fff; border-top: 9px solid #fff;
        border-right: 5px solid transparent; border-bottom: 5px solid transparent;
        transform: rotate(-8deg) translate(1px, 1px); transform-origin: 0 0;
      }
    `;
    document.addEventListener("DOMContentLoaded", () => {
      document.head.appendChild(style);
      const el = document.createElement("div");
      el.id = "__rec_cursor";
      document.body.appendChild(el);
      window.__recCursor = el;
      window.addEventListener("mousemove", (e) => {
        el.classList.add("show");
        el.style.left = e.clientX + "px";
        el.style.top = e.clientY + "px";
      }, true);
      window.addEventListener("mousedown", () => el.classList.add("click"), true);
      window.addEventListener("mouseup", () => el.classList.remove("click"), true);
    });
  });
}

/** 平滑移动鼠标（多段小步），视频里看起来自然。 */
export async function glide(page, x, y, steps = 18) {
  const cur = await page.evaluate(() => {
    const el = window.__recCursor;
    if (!el) return { x: 0, y: 0 };
    return { x: parseFloat(el.style.left) || 0, y: parseFloat(el.style.top) || 0 };
  });
  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    const nx = cur.x + (x - cur.x) * t;
    const ny = cur.y + (y - cur.y) * t;
    await page.mouse.move(nx, ny);
    await page.waitForTimeout(10);
  }
}

/** 移到元素中心并点击（带平滑移动 + 按下延时，便于看清）。 */
export async function clickAt(page, locator) {
  const box = await locator.boundingBox();
  if (!box) throw new Error("元素不可见，无法点击");
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await glide(page, cx, cy);
  await page.waitForTimeout(120);
  await page.mouse.down();
  await page.waitForTimeout(120);
  await page.mouse.up();
  await page.waitForTimeout(200);
}

/** 悬停某元素（用于展示 hover 态）。 */
export async function hover(page, locator) {
  const box = await locator.boundingBox();
  if (!box) return;
  await glide(page, box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForTimeout(400);
}

/** 滚动到某元素并停留（视频里能看清）。 */
export async function scrollTo(page, selector, offset = -80) {
  await page.evaluate(([s, o]) => {
    const el = document.querySelector(s);
    if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
    window.scrollBy(0, o);
  }, [selector, offset]);
  await page.waitForTimeout(700);
}

/** 等待主界面渲染完成（避开白屏与启动页），再开始录屏动作。 */
export async function waitReady(page) {
  await page.locator(".brand h1").waitFor({ state: "visible", timeout: 15000 });
  // 环境检查是异步的，等它填完，避免录到"检查中"
  await page.waitForFunction(() => {
    const els = document.querySelectorAll(".env-item, .check-item, [class*='env']");
    return els.length > 0;
  }, { timeout: 8000 }).catch(() => {});
  await page.waitForTimeout(400);
}

/** 开头停顿（让观众看清初始画面），结尾停顿（让观众消化）。 */
export const HOLD_START = 900;
export const HOLD_END = 900;
