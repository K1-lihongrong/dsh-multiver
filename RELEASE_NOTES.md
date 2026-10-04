## DSH 版本管理器 v0.2.2

本版本新增**查看 dsh 版本更新说明**与**启动崩溃主动提示**，并改进卸载体验。

### ✨ 新增

- **查看版本更新说明**：点版本号即可就地展开该版本的更新说明（数据来自 dsh 上游 GitHub Releases），
  含「在浏览器打开原文」按钮。装之前就能了解每个版本改了什么。
- **dsh 启动后崩溃主动提示**：若 dsh 启动后立即崩溃，管理器会在几秒后主动提示并附上真实错误
  （不再让你对着"拒绝连接"发懵）。

### 🔧 变更

- **卸载改为「秒删」**：点「卸载」后版本**立即**从列表消失（目录先 rename 到回收站、后台再删），
  Windows 上不再需要等几十秒。

### 🐛 修复

- dsh 启动失败时**展示真实错误**（而非笼统的"输出格式可能已变化"）
- 进程表锁中毒时不再 panic，应用更稳
- Linux 无桌面环境时，桌面快捷方式给出清晰错误提示

### 📖 文档

- README / 使用手册同步 Linux 支持；新增《征求协助》征求非 Windows 平台的真机验证

### 📦 下载

| 平台 | 文件 | 说明 |
| :--- | :--- | :--- |
| **Windows** | `dsh-multiver.exe` | 便携版，双击运行（需 WebView2） |
| **Linux（通用）** | `dsh-multiver_*_amd64.AppImage` | 免安装，`chmod +x` 后运行 |
| **Linux（Debian/Ubuntu）** | `dsh-multiver_*_amd64.deb` | `sudo dpkg -i` 安装 |

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
