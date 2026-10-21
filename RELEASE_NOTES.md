## DSH 版本管理器 v0.3.0

> ⚠️ **破坏性变更**：CLI 从 `--flag` 风格改为子命令风格。旧语法已移除。

### 🔧 CLI 子命令风格（破坏性变更）

```bash
# 旧（v0.2.x）        新（v0.3.0）
dsh-multiver --list   →  dsh-multiver list
dsh-multiver --install 0.1.7  →  dsh-multiver install 0.1.7
dsh-multiver --set-default 0.1.7  →  dsh-multiver set-default 0.1.7
```

- 全部 21 个命令改为子命令风格。
- **修饰符语法不变**：`--json` / `--dry-run` / `--force` / `--registry <url>` / `--off`。
- **GUI 启动不变**（不带子命令时启动界面）。
- **内部参数不变**：`--launch-version` / `--debug-progress`。

> 📖 **升级前请读 [CLI 迁移指南](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/CLI迁移指南-v0.3.0.md)**（旧→新对照表 + 脚本批量替换）。

### 🧹 结构重构（技术债全部清理）

五个超大文件拆分为子模块，**外部接口零变化**：

| 原文件 | 行数 | 拆分后 |
| :--- | :--- | :--- |
| `cli.rs` | 1750 | `cli/` 6 文件（parse / output / help / console / mod / commands） |
| `commands.rs` | 1687 | `commands/` 12 文件（launch / scan / version / state / plugins / env / config_ops / isolated / update / backup / misc / mod） |
| `versions.rs` | 1079 | `versions/` 4 子模块（install / size / list / isolated） |
| `launcher.rs` | 979 | `launcher/` 3 子模块（process / diagnose / shortcut） |
| `runtime.rs` | 533 | `runtime/` 3 文件（bundled / webview2 / mod） |

另抽出 `paths.rs`（137 行纯路径函数，与 Tauri 无关）。单文件全部 ≤400 行，职责清晰。

### 🐛 修复

- **中文 Windows 下子进程错误信息乱码**：控制台代码页为 936（GBK），此前按 UTF-8 解码导致中文变替换字符（如刷新版本列表显示 `pnpm ����×��∫`）。现改为先 UTF-8 严格解码、失败按 GBK，错误信息可正常阅读。
- **占用扫描缓存清理**：新增 `prune`，卸载的版本不再留下永远无法命中的缓存条目。

### 🔌 dsh 插件管理（双向桥 B1）

已安装版本的「插件」按钮 → 管理该版本的 dsh 插件（list / install / remove），通过 dsh 自带 CLI 执行。

- 前提：该版本已在 dsh 里启动过（才有 profile 目录）。
- 双向桥的 dsh 侧（B2-B4）已交接给 dsh 开发方，见 [交接文档](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/dsh双向桥-交接文档.md)。

### 说明

- 这是**大版本**（语义化版本规范：破坏性变更）。
- **功能变化**：CLI 语法改为子命令风格（破坏性）；修复了子进程错误信息乱码（D16）。
- **不变的部分**：数据格式、存储结构、GUI 功能、DSH_HOME 语义、隔离/共享行为。
- **双向桥 B2-B4**（dsh 侧插件）已交接 dsh 开发方，不在本版本范围。

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
