## DSH 版本管理器 v0.2.0

**本版本首次提供 Linux 构建**，并与 Windows 功能同步。v0.1.8 → v0.2.0 的主要变化如下。

### 🐧 新增：Linux 支持（预览）

- 与 Windows 功能对齐：安装 / 卸载 / 运行 / 数据隔离 / CLI / 终端短命令 / 桌面快捷方式
- **进程清理跨平台**：
  - Windows：Job Object（管理器退出即杀 dsh 进程树）
  - Linux：进程组 + `PDEATHSIG`（管理器正常关闭或**被强杀**，dsh 进程都会被清理，无孤儿）
- **终端短命令**：Linux 下生成 `~/.local/bin/dsh`（POSIX sh 脚本）
- **桌面快捷方式**：Linux 下生成 `.desktop` 文件
- 已在 Debian 13 上完成编译、单元测试、GUI 启动、CLI、转发脚本、进程清理的验证

> Linux 构建产物为 **AppImage（免安装，推荐）** 与 **.deb（Debian/Ubuntu）**。
> AppImage 需先 `chmod +x dsh-multiver_*.AppImage` 再运行。

### 📋 新增：运行日志（可观测性）

- **会话日志**：每次「运行」某版本，完整记录该次 dsh 的 stdout/stderr，落在 `<数据根>/logs/session-*.log`（保留最新 20 份）
- **主进程错误** `logs/app.log`、**前端错误** `logs/frontend.log`
- 遇到问题时可据此定位，详见 [问题日志查看指南](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/问题日志查看指南.md)

### 🎨 变更

- **界面显示自身版本号**：顶栏标题旁显示 `dsh-multiver vX.Y.Z`
- **新应用图标**：改用独立的「层叠版本」图标（不再使用 dsh 官方 logo 图形）
- **启动超时 30s → 90s**：缓解 dsh 首次（冷）启动较慢时"首次失败、再点就成功"的问题

### 🔧 工程

- 新增 41 个 Rust 单元测试；CI（Windows + Ubuntu 矩阵）在 push/PR 时自动跑测试
- 跨平台代码通过 `cfg` 分支隔离，Windows / Linux 各自编译验证

### 📦 下载

| 平台 | 文件 | 说明 |
| :--- | :--- | :--- |
| **Windows** | `dsh-multiver.exe` | 便携版，双击运行（需 WebView2） |
| **Linux（通用）** | `dsh-multiver_*_amd64.AppImage` | 免安装，`chmod +x` 后运行 |
| **Linux（Debian/Ubuntu）** | `dsh-multiver_*_amd64.deb` | `sudo dpkg -i` 安装 |

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
