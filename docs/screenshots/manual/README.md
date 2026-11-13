# 手册配图（五语动态 WebP）

> 本目录的 WebP 由 Playwright 录屏自动生成，对应五份 `使用手册` 的插图。

## 文件命名

| 文件名 | 语言 |
| :--- | :--- |
| `01-main.webp` | 简体中文 |
| `01-main.zh-TW.webp` | 繁體中文 |
| `01-main.en.webp` | English |
| `01-main.ja.webp` | 日本語 |
| `01-main.ko.webp` | 한국어 |

当前 **17 个 multiver 模块 × 5 语 = 85 个 WebP**；另有 `16-plugin-install.webp` 一份中文例外（dsh 自身插件页，界面语言由 dsh 决定）。

## 为什么用动态 WebP

静态截图无法展示交互反馈（悬停描边、按下缩放、面板展开、进度推进）。v0.3.4 统一了全站按钮动效，静态图看不到。

WebP（有损 q75）体积约为 GIF 的 38.6%，画质肉眼无差，GitHub 与现代浏览器均支持动态 WebP。

## 字幕

每张图底部有讲解字幕，随步骤切换。字幕时间轴由 spec 里的 `cue(zh, en, ja)` 记录；缺 `zh-TW`/`ko` 时，转码脚本用本地 Ollama 翻译并缓存到 `%TEMP%/dsh-multiver/cues/i18n-cache.json`。

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

## 怎么重新生成

前置：ffmpeg（全功能构建，需含 `subtitles` 滤镜与 libass）、本地 Ollama。

```powershell
cd dsh-multiver
.scriptsecord-manual.ps1 -Lang zh,en,ja,ko,zh-TW
```

脚本按语言依次：设 `REC_LANG` → 录屏 → `build-gifs --lang=xx`。**界面语言必须与输出语言一致**，不能一次录屏硬出五语。

只重录某几张：

```powershell
.scriptsecord-manual.ps1 -Lang ko -Only 09-isolated,17-instance-panel
```

ffmpeg 不在默认路径时用环境变量指定：`FFMPEG=/path/to/ffmpeg node e2e/record/build-gifs.mjs --lang=zh`。

## 实现要点

| 项 | 说明 |
| :--- | :--- |
| 录屏配置 | `playwright.record.config.js`（1280×800、workers=1、video on） |
| 界面语言 | `mockTauri` 的 `lang` 参数 / `REC_LANG`，写入 `localStorage` |
| 按钮定位 | `e2e/record/i18n.js` 的 `T(key)` 直接读前端词条 |
| 录屏工具 | `e2e/record/_rec.js`：假光标、平滑移动、按元素点击、就绪等待、字幕时间轴 |
| mock 后端 | `e2e/helpers.js` 的 `mockTauri`（含事件通道 `window.__mockEmit`） |
| 转码参数 | `-ss 0.8` 裁启动白屏、`fps=6`、宽度 960、底部 44px 黑边烧 ASS 字幕、`libwebp_anim -quality 75` |
| 字体 | 简中：Microsoft YaHei；繁中：Microsoft JhengHei；英：Segoe UI；日：Yu Gothic；韩：Malgun Gothic |

## 例外与坑

- **16-plugin-install** 是 dsh 自身插件页，无法由 multiver 切换语言；保留中文图，翻译手册中图下注明。
- 10-topbar-dot 的 dsh 背景同样是 dsh 自身界面；顶栏可见文字是版本号/图标，语言中立。
- **不要用 `palettegen/paletteuse`**：本机 ffmpeg 9.0 对单帧序列统计异常，会输出 645 字节空调色板，成品全白。
- **Playwright 自带的 ffmpeg 是极简构建**，没有 WebP 编码器与 subtitles 滤镜，不能用于转码。
- **`subtitles` 滤镜路径**：Windows 下含中文的绝对路径会失败。转码前先把 webm 复制到纯 ASCII 临时目录，并用相对文件名 + `cwd`。
- **字幕时间轴要减 `-ss` 的偏移**：ASS 里的时间点是相对完整视频的，转码裁掉开头 0.8 秒后要同步减 0.8。
- **常规 E2E 会清空 `test-results`**：跑完 `pnpm test:e2e` 后录屏文件就没了，要重新录。
- **`--lang=all` 已移除**：五语必须分别录屏，否则界面与字幕语言不一致。

## 顶栏圆点（10）

`topbar.js` 是后端注入的纯 DOM 脚本，不依赖 dsh 后端。`10-topbar-dot.spec.js` 直接注入它并模拟悬停/点击，录出 dot → pill → 面板三态。
