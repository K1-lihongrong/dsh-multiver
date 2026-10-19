# E2E 测试（Playwright）

> v0.2.14 引入。测**纯前端**（渲染、交互、状态），**不测 Rust 后端**。

## 为什么

前端改动（尤其 UI）过去靠人手点验证，慢且易漏。E2E 自动跑，把"白屏/元素缺失/文案错/交互失效"类问题在提交前挡掉。

## 用法

```bash
# 首次（或依赖更新后）：装浏览器（约 118MB，若已有缓存会跳过）
npx playwright install chromium-headless-shell

# 跑测试（会自动 build + 起 preview）
pnpm test:e2e

# 可视化调试
pnpm test:e2e:ui
```

## 限制（重要）

- **mock Tauri**：测试环境是纯 web（vite preview），无 Rust 后端。所有 `invoke` 由 `e2e/helpers.js` 的 `mockTauri` 模拟。
- **不测**：Rust 命令真实行为、真实 dsh 启动、动画流畅度、视觉美感。这些仍需人验收。
- **函数不能传**：`page.addInitScript` 参数走结构化克隆，**函数会丢**。自定义 mock 用**字符串函数体**：
  ```js
  extra: { run_version: "(args) => new Promise(r => setTimeout(() => r('ok'), 800))" }
  ```

## 文件

| 文件 | 内容 |
| :--- | :--- |
| `helpers.js` | `mockTauri`（Tauri mock）+ `makeVersion`（构造版本项） |
| `smoke.spec.js` | 渲染不白屏、列表渲染 |
| `launch.spec.js` | 启动态（快捷方式/运行按钮）、静态骨架 |
| `dialog.spec.js` | 共享 home 确认对话框（继续/我知道了/取消） |
| `i18n.spec.js` | 4 语言切换、启动态文案 |
