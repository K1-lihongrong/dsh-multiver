## DSH 版本管理器 v0.2.9

### ✨ 运行时适配：环境不齐也能用起来

此前若电脑缺 Node.js / pnpm / WebView2，用户需自己找、自己装。现在改为**按需自动准备**：

- **便携 Node / pnpm 自动下载**：检测到系统缺失时，下载到数据根内 `<root>/tools/`，不污染系统、不改 PATH
- **自动注入**：装 / 跑 dsh 时把便携运行时目录注入子进程 PATH，dsh 直接用便携 Node 运行
- **环境检查联动**：系统缺但便携就绪时，检查项显示「将使用便携 Node / pnpm」并通过
- **界面按钮**：环境检查面板「自动准备运行时」→ 下载（带进度）→ 完成即用
- **CLI**：`dsh-multiver --setup-runtime`（`--dry-run` 只报告，`--json` 机读）

**设计原则**：exe 体积不变（约 9MB）、一切按需下载、不偷偷下、不污染系统、仅 Windows。

### ✨ WebView2 早期检测引导

缺 WebView2 时（管理器根本起不来），在 Tauri 初始化前用 **Win32 原生弹窗**引导：

- [自动下载并安装] → 下 Evergreen Bootstrapper（约 2MB）并拉起安装
- [打开下载页] → 打开微软官方下载页

### 🔧 修复：npm 源连通检查回退便携 pnpm

系统无 pnpm 但便携就绪时，环境检查的「npm 源连通」此前会误报失败。现在回退用便携 pnpm 查询。

### 说明

- **无破坏性变更**：CLI 默认输出、界面默认行为、隔离/共享语义均不变
- 便携运行时仅约 80MB，放数据根内，可随时删除（下次用时重下）
- 平台：Windows（Linux 保持引导）

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
