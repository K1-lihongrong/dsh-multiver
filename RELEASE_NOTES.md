## DSH 版本管理器 v0.2.3

本版本修复了**任务栏图标颠倒**与**更新说明加载慢**两个问题，并完成了前端结构调整。

### 🐛 修复

- **任务栏图标各归各位**：管理器窗口显示管理器图标、运行中的 dsh 窗口显示 dsh 图标。
  此前两者是颠倒的（管理器显示 dsh 图标、dsh 窗口显示管理器图标），根源在于进程的
  AppUserModelID 设置时机太晚——现在已修正。
- **更新说明加载变快**：同一个版本的更新说明，看过一次后，无论从「可用版本」还是
  「已安装版本」再点，都能立即显示（此前每次都要重新联网拉取）。
- **数据根目录**：自动去掉用户输入的首尾空格，避免生成异常的带空格目录。

### 🔧 变更

- **桌面快捷方式使用 dsh 官方图标**（此前用的是管理器图标）。
- **路径设置新增「展开全部目录」**：默认显示 5 个核心目录，展开后可查看
  logs / webview / trash / assets 等全部目录。

### ♻️ 重构（无功能变化）

- 前端 `App.vue` 从 1336 行拆分到 173 行，按职责拆为 6 个组件 + 3 个 composable +
  全局样式文件。**纯结构调整，功能与外观不变。**

### 📦 下载

| 平台 | 文件 | 说明 |
| :--- | :--- | :--- |
| **Windows** | `dsh-multiver.exe` | 便携版，双击运行（需 WebView2） |
| **Linux（通用）** | `dsh-multiver_*_amd64.AppImage` | 免安装，`chmod +x` 后运行 |
| **Linux（Debian/Ubuntu）** | `dsh-multiver_*_amd64.deb` | `sudo dpkg -i` 安装 |

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
