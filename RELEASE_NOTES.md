## DSH 版本管理器 v0.2.1

本版本为 v0.2.0 之后的**维护性更新**：以内部重构与工程质量改进为主，含一处用户可见的体验改进。

### 🔧 变更

- **超时错误提示更实用**：启动超时或无法解析地址时，错误信息**直接给出日志目录的完整路径**，
  便于直接定位问题日志（无需自己寻找 `logs/` 在哪）
- **代码结构优化（内部）**：`lib.rs` 从 971 行降至 190 行——窗口/平台辅助移入 `window.rs`，
  全部 Tauri 命令移入 `commands.rs`。纯结构调整，**不影响任何功能行为**

### ✅ 测试

- 单元测试从 41 个增至 **57 个**：新增命令层纯逻辑覆盖
  （`build_topbar_script` / `resolve_home` / `Config::apply_install_result` /
  `Config::remove_version_refs` / `Dirs::resolve`）
- Windows / Linux 两平台编译与测试均通过、零警告

### 📦 下载

| 平台 | 文件 | 说明 |
| :--- | :--- | :--- |
| **Windows** | `dsh-multiver.exe` | 便携版，双击运行（需 WebView2） |
| **Linux（通用）** | `dsh-multiver_*_amd64.AppImage` | 免安装，`chmod +x` 后运行 |
| **Linux（Debian/Ubuntu）** | `dsh-multiver_*_amd64.deb` | `sudo dpkg -i` 安装 |

> Linux 产物已在 Debian 13 上完成实机验证（AppImage 与 deb 的 CLI 均可正常运行）。

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
