# 手册配图（GIF）

> 本目录的 12 个 GIF 由 Playwright 录屏自动生成，对应 `使用手册.md` 的插图。

## 为什么用 GIF

静态截图无法展示交互反馈（悬停描边、按下缩放、面板展开、进度推进）。v0.3.4 统一了全站按钮动效，静态图看不到。

## 怎么重新生成

前置：ffmpeg（全功能构建，需含 GIF 编码器）。

```bash
# 1. 录屏（12 个 spec，约 1.5 分钟）
pnpm exec playwright test --config=playwright.record.config.js

# 2. 转码为 GIF（输出到本目录）
node e2e/record/build-gifs.mjs

# ffmpeg 不在默认路径时用环境变量指定
FFMPEG=/path/to/ffmpeg node e2e/record/build-gifs.mjs
```

## 实现要点

| 项 | 说明 |
| :--- | :--- |
| 录屏配置 | `playwright.record.config.js`（1280×800、workers=1、video on） |
| 录屏工具 | `e2e/record/_rec.js`：假光标、平滑移动、按元素点击、就绪等待 |
| mock 后端 | `e2e/helpers.js` 的 `mockTauri`（含事件通道 `window.__mockEmit`） |
| 转码脚本 | `e2e/record/build-gifs.mjs`（`-ss 0.8` 裁掉启动白屏，`fps=10`，宽度 960） |

## 两个坑

- **不要用 `palettegen/paletteuse` 滤镜**：本机 ffmpeg 9.0 对单帧序列统计异常，会输出 645 字节的空调色板，成品全白。直接编码正常。
- **Playwright 自带的 ffmpeg 是极简构建**，没有 GIF 编码器，不能用于转码。

## 顶栏圆点（10）

`topbar.js` 是后端注入的纯 DOM 脚本，不依赖 dsh 后端。`10-topbar-dot.spec.js` 直接注入它并模拟悬停/点击，录出 dot → pill → 面板三态。
