## DSH 版本管理器 v0.1.8

### ✨ 新增：全套无头 CLI

带任一 CLI 参数即**不启动图形界面**，执行完按退出码退出（成功 `0` / 失败 `1`），方便脚本化、批处理和 CI：

```bash
dsh-multiver --list                                  # 列出已安装版本（默认版本以 "* " 标记）
dsh-multiver --install 0.2.0-rc.2                    # 安装指定版本
dsh-multiver --install 0.2.0-rc.2 --registry <url>   # 指定 npm 源安装
dsh-multiver --uninstall 0.1.5-rc.3                  # 卸载
dsh-multiver --set-default 0.2.0-rc.2                # 设为默认版本（重生成终端 dsh 命令）
dsh-multiver --maintenance --cleanup                 # 清理孤立缓存
dsh-multiver --maintenance --prune                   # 回收依赖仓库
dsh-multiver --maintenance                           # 两者都做
dsh-multiver --help                                  # 帮助
dsh-multiver --version                               # 版本号
```

- **控制台自动接入**：release 版本身没有控制台窗口，CLI 模式会自动附加到父控制台并切换为 UTF-8 输出，因此在 cmd / PowerShell 里**直接运行就能看到输出**；若把输出重定向到文件或管道，则保持原样不干扰
- **输出可管道**：`--list` 一行一个版本号；安装进度走 stderr 且仅在真实终端下显示，stdout 始终干净

### 🔧 变更

- 未知参数（如打错的 `--instal`）**直接报错退出**，不再默默弹出图形界面
- 清理 `get_manager_dir` 死代码

### 说明

- 破坏性命令（install / uninstall / set-default）**无交互确认**，靠精确参数保护，适合脚本调用
- 不带任何 CLI 参数时，行为与之前完全一致：启动图形界面

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
