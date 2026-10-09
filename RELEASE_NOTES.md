## DSH 版本管理器 v0.2.5

### ✨ 新增：CLI `--json`（结构化输出）

给脚本 / CI 用的机读输出。加 `--json` 即可，stdout **只有 JSON**：

```bash
dsh-multiver --list --json        # 版本对象数组（含 is_default / isolated / installed_at）
dsh-multiver --which --json       # {"version":"...","path":"..."}
dsh-multiver --version --json     # {"version":"0.2.5"}
dsh-multiver --install X --dry-run --json   # 预览对象
```

> **只影响格式，不改行为**。不带 `--json` 时的输出与之前**完全一致**——现有脚本不受影响。

### ✨ 新增：CLI `--which`

输出默认版本 dsh 入口的**绝对路径**，脚本里很方便：

```bash
DSH="$(dsh-multiver --which)"
"$DSH" --version
```

无默认版本 / 入口不存在 → 退出码 1。

### 🌏 新增语言：繁體中文 + 日本語

界面语言从 2 种增至 **4 种**：

| 语言 | |
| :--- | :--- |
| 简体中文 | ✅ |
| **繁體中文** | 🆕 |
| English | ✅ |
| **日本語** | 🆕 |

系统语言识别会自动匹配（繁体区 zh-Hant / zh-TW / zh-HK → 繁體中文）。

### 说明

本次为**纯功能增量**，无破坏性变更。

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
