## DSH 版本管理器 v0.2.7

### ✨ 新增：CLI 补全 3 条命令 + 导出

| 命令 | 用途 |
| :--- | :--- |
| `--shortcut <版本>` | 无头创建桌面快捷方式 |
| `--copy-shared <版本>` | 把共享 home 数据复制到某隔离版本 |
| `--clear-isolated <版本>` | 清空某隔离版本的独立 home |
| `--export [<文件>]` | 导出配置 + 版本清单（换机迁移用） |

均支持 `--dry-run` / `--json`。

### ✨ 新增：界面「导出配置」按钮

路径设置区一键导出配置 + 版本清单为 JSON（含保存对话框），换机迁移时参考。

### 🌏 改进：环境检查项名跟随语言

「环境检查」面板的检查项名（Node.js / pnpm / 根目录可写 / 磁盘空间 / npm 源连通）
现在随界面语言切换（简体 / 繁體 / English / 日本語）。

### 🔧 改进：`--doctor` 增加孤立缓存检查

新增「孤立 webview 缓存」检查项：报告版本已卸载但 WebView 数据残留的情况
（只报告不误删，可用 `--clean` 清理）。

### 说明

- **无破坏性变更**：CLI 默认输出、界面默认行为均不变
- 本次为**功能补完 + 体验优化**

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
