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

### 🧹 结构重构

- **拆分 `cli.rs`（1750 行）→ `cli/` 模块**：
  - `parse.rs`（解析）、`output.rs`（输出）、`help.rs`（帮助）、`console.rs`（控制台）、`mod.rs`（入口）。
  - `commands/`：`version` / `default` / `store` / `env` / `misc`。
  - 单文件 ≤400 行，职责清晰，便于维护。

### 说明

- 这是**大版本**（语义化版本规范：破坏性变更）。
- 未改任何功能行为（只是命令行语法）。
- 数据格式、存储结构、GUI 功能**完全不变**。

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
