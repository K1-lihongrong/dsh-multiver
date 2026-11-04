# 手册配图（动态 WebP）

> 本目录的 WebP 由 Playwright 录屏自动生成，对应 `使用手册.md` 的插图。

## 文件命名

| 文件名 | 语言 |
| :--- | :--- |
| `01-main.webp` | 简体中文（手册引用的是这份） |
| `01-main.en.webp` | English（暂未生成） |
| `01-main.ja.webp` | 日本語（暂未生成） |

当前 14 张图，仅简体中文版，共约 2.9 MB，单张 33~566 KB。多语言版待后续专项处理（GAP-019）。

## 为什么用动态 WebP

静态截图无法展示交互反馈（悬停描边、按下缩放、面板展开、进度推进）。v0.3.4 统一了全站按钮动效，静态图看不到。

WebP（有损 q75）体积约为 GIF 的 38.6%，画质肉眼无差，GitHub 与现代浏览器均支持动态 WebP。

## 字幕

每张图底部有讲解字幕，随步骤切换。字幕时间轴由 spec 里的 `cue(zh, en, ja)` 记录：

```js
import { startCues, cue, saveCues } from "./_rec.js";

test("...", async ({ page }) => {
  startCues();               // 在 goto 之前
  // ... 关键动作
  cue("中文讲解", "English", "日本語");
  // ...
  saveCues("01-main");       // 与文件名一致
});
```

时间轴写入 `%TEMP%/dsh-multiver/cues/<key>.json`，转码时按 `--lang` 生成对应 ASS 烧入 WebP（默认只出简体中文）。

## 怎么重新生成

前置：ffmpeg（全功能构建，需含 `subtitles` 滤镜与 libass）。

```bash
# 1. 清空旧时间轴（可选）
rm -rf "$TEMP/dsh-multiver/cues"

# 2. 录屏（13 个 spec，约 2.5 分钟）
pnpm exec playwright test --config=playwright.record.config.js

# 3. 转码为 WebP（默认只出简体中文，输出到本目录）
node e2e/record/build-gifs.mjs

# 生成全部语言（多语言版待专项处理，当前通常不跑）
node e2e/record/build-gifs.mjs --lang=all
```

ffmpeg 不在默认路径时用环境变量指定：`FFMPEG=/path/to/ffmpeg node e2e/record/build-gifs.mjs`。

## 实现要点

| 项 | 说明 |
| :--- | :--- |
| 录屏配置 | `playwright.record.config.js`（1280×800、workers=1、video on） |
| 录屏工具 | `e2e/record/_rec.js`：假光标、平滑移动、按元素点击、就绪等待、字幕时间轴 |
| mock 后端 | `e2e/helpers.js` 的 `mockTauri`（含事件通道 `window.__mockEmit`） |
| 转码参数 | `-ss 0.8` 裁启动白屏、`fps=6`、宽度 960、底部 44px 黑边烧 ASS 字幕、`libwebp_anim -quality 75` |
| 字体 | 中：Microsoft YaHei，英：Segoe UI，日：Yu Gothic |

## 踩过的坑

- **不要用 `palettegen/paletteuse` 滤镜**：本机 ffmpeg 9.0 对单帧序列统计异常，会输出 645 字节的空调色板，成品全白。直接编码正常。
- **Playwright 自带的 ffmpeg 是极简构建**，没有 WebP 编码器与 subtitles 滤镜，不能用于转码。必须用全功能构建（含 libwebp_anim、libass）。
- **`subtitles` 滤镜路径**：Windows 下含中文的绝对路径会失败。转码前先把 webm 复制到纯 ASCII 临时目录，并用相对文件名 + `cwd`。
- **字幕时间轴要减 `-ss` 的偏移**：ASS 里的时间点是相对完整视频的，转码裁掉开头 0.8 秒后要同步减 0.8。
- **常规 E2E 会清空 `test-results`**：跑完 `pnpm test:e2e` 后录屏文件就没了，要重新录。

## 顶栏圆点（10）

`topbar.js` 是后端注入的纯 DOM 脚本，不依赖 dsh 后端。`10-topbar-dot.spec.js` 直接注入它并模拟悬停/点击，录出 dot → pill → 面板三态。
