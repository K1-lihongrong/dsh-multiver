# CLI 增强规划（v0.2.5+ 候选）

> 编写日期：2026-10-08 · 基线：v0.2.4
> 本文只做**规划**，不实现。实现前应先在 `docs/开发缺口.md` 登记（见文末）。

---

## 一、现状（v0.2.4）

### 已有命令（7 条）

| 命令 | 类别 |
| :--- | :--- |
| `--list` | 查询（本地） |
| `--install <版本> [--registry <url>]` | 操作 |
| `--uninstall <版本>` | 操作 |
| `--set-default <版本>` | 操作 |
| `--maintenance [--cleanup\|--prune]` | 运维 |
| `--help` / `-h` | 元 |
| `--version` / `-V` | 元 |

**修饰符**：`--dry-run`（作用于 4 条破坏性命令）

### 已有能力（后端已实现，CLI 未暴露）

| 后端函数 | 位置 | GUI 用 | CLI 用 |
| :--- | :--- | :--- | :--- |
| `versions::list` | versions.rs | ✅ | ✅（`--list`） |
| `versions::install` | versions.rs | ✅ | ✅ |
| `versions::uninstall_fast` | versions.rs | ✅ | ✅ |
| `versions::exists` | versions.rs | ✅ | ❌ |
| `versions::dir_size` | versions.rs | ✅ | ❌ |
| `versions::version_size_detail` | versions.rs | ✅ | ❌ |
| `versions::isolated_home` | versions.rs | ✅ | ❌ |
| `versions::copy_shared_to_isolated` | versions.rs | ✅ | ❌ |
| `versions::clear_isolated` | versions.rs | ✅ | ❌ |
| `maintenance::cleanup_orphan_webviews` | maintenance.rs | ✅ | 部分（`--cleanup`） |
| `maintenance::cleanup_trash` | maintenance.rs | ✅ | ❌ |
| `maintenance::run_store_prune` | maintenance.rs | ✅ | ✅（`--prune`） |
| `envcheck::run_all` | envcheck.rs | ✅ | ❌ |
| `actions::list_remote`（npm 查可用版本） | actions.rs | ✅ | ❌ |
| `commands::create_shortcut` | commands.rs | ✅ | ❌ |
| `commands::set_isolated` | commands.rs | ✅ | ❌ |

**结论**：**多条后端能力已就绪，只差 CLI 入口** —— 这是低垂的果实。

### 现状短板

1. **查询能力弱**：只能 `--list`（纯版本号），查不到"某版本详情"
2. **输出不可机读**：人类可读文本，脚本解析脆弱（如 `* 0.1.7` 要剥前缀）
3. **能力不全**：隔离 / 快捷方式 / 环境检查 / 查可用版本 都只能 GUI 做
4. **无诊断命令**：出问题只能看日志

---

## 二、增强方向（按性价比排序）

### 🥇 P0：结构化输出 `--json`

**为什么最优先**：**一条改动，所有命令受益**。

| 命令 | 加 `--json` 后 |
| :--- | :--- |
| `--list --json` | `["0.1.7","0.1.8"]` 或带元数据 `[{"version":"0.1.7","default":true}]` |
| `--info X --json` | 完整对象 |
| `--env --json` | 检查项数组 |
| `--versions --json` | 可用版本数组 |

**设计**：
- `--json` 是**修饰符**（与 `--dry-run` 同类）
- 只影响**输出格式**，不改行为
- 非 JSON 时保持现状（**不破坏兼容**）
- 错误也走 JSON：`{"error":"..."}`，退出码仍 1

**实现要点**：
- `cli.rs` 加 `json: bool`（从 `lib.rs` 传入，与 `dry_run` 同处）
- 各 `cmd_*` 分支：`if json { println!("{}", serde_json::to_string(...)) } else { 现有输出 }`
- 复用现有的 `Serialize` 结构（`VersionInfo` / `CheckItem` / `SizeInfo` 已 `derive(Serialize)`）

**成本**：小（~半天）。**收益**：大。

---

### 🥇 P0：`--which`

**用途**：脚本里拿"默认版本 dsh 的入口路径"。

```bash
# 现状：脚本要自己拼路径
DSH="$ROOT/versions/$(dsh-multiver --list | grep '^\*' | cut -d' ' -f2)/node_modules/.bin/dsh"

# 目标：
DSH="$(dsh-multiver --which)"
```

**输出**：默认版本的 `node_modules/.bin/dsh`（或 `.cmd`）**绝对路径**。

**边界**：
- 无默认版本 → 退出码 1 + stderr 提示
- 可选 `--which --dir` 输出**版本目录**而非入口

**成本**：极小（~1 小时）。**收益**：脚本高频。

---

### 🥈 P1：`--info <版本>`

**用途**：查单个版本详情。

**输出**（人读）：
```
版本:     0.1.7
路径:     E:\dsh\versions\0.1.7
默认:     是
隔离:     否（共享 home）
安装于:   2026-09-30
占用:     1.38 GB（复用 1.10 GB / 独占 288 MB）
```

**`--json` 输出**：直接序列化 `VersionInfo` + `SizeInfo`。

**成本**：小（~半天，复用 `versions::list` + `version_size_detail`）。**收益**：补全查询。

> 可选：`--info` 不带参数 → 输出**所有**版本详情（等价 `--list --verbose`）。

---

### 🥈 P1：`--versions`（列可用版本）

**用途**：无头环境查"能装哪些版本"（现在只能 GUI）。

**输出**：一行一个版本号（最新在前），与 GUI 的「可用版本」一致。

**实现**：直接调 `actions::list_remote`。

**成本**：小（~1 小时）。**收益**：中（补全"远端查询"）。

---

### 🥈 P1：`--env`

**用途**：无头环境检查（部署前预检 / 排障）。

**输出**（人读）：
```
[✓] Node.js       v22.19.0（满足 22.19+ / 24+）
[✓] pnpm          12.10.1
[✓] 根目录可写    E:\dsh
[✓] 磁盘空间      可用 128.4 GB
[✗] npm 源连通    连接超时
```

**退出码**：有**致命项**失败 → 1；否则 0（便于脚本 `if dsh-multiver --env; then ...`）。

**`--json`**：序列化 `Vec<CheckItem>`。

**成本**：小（~半天，复用 `envcheck::run_all`）。**收益**：中。

---

### 🥉 P2：`--isolate <版本> [--on|--off]`

**用途**：无头开关数据隔离。

**实现**：复用 `commands::set_isolated` 的逻辑（需从 `commands.rs` 抽出可复用函数，或直接操作 `cfg.isolated_versions`）。

**成本**：小-中（要处理"设隔离后重生成转发脚本"）。**收益**：中（补全 GUI 能力）。

---

### 🥉 P2：`--clean`

**用途**：`--maintenance` 的超集 —— 额外清 **trash**（回收站）。

**现状**：`--maintenance --cleanup` 只清孤立 webview，**不清 trash**（GUI 手动维护才清）。

**成本**：极小（`maintenance::cleanup_trash` 已存在）。**收益**：中。

> 或者：让 `--maintenance --cleanup` **就包含 trash**（行为变更，需权衡）。

---

### 🥉 P2：`--doctor`

**用途**：一键诊断（排障用）。

**检查项**：
- 数据根可写？
- 各目录是否存在（versions/store/home/webview/logs）？
- 有无 broken 版本？
- 有无残留进程（`procreg` 登记表）？
- 磁盘空间？
- Node/pnpm 版本？
- 最近错误日志摘要？

**输出**：逐项 ✓/✗ + 建议动作。

**成本**：中（聚合多个检查）。**收益**：中（排障友好）。

---

### ⚪ P3：`--shortcut <版本>`

无头创建桌面快捷方式。复用 `launcher::create_desktop_shortcut` / `commands::create_shortcut`。

**成本**：小。**收益**：低（GUI 做得更好；且"无头环境"通常无桌面）。

---

### ⚪ P3：`--copy-shared <版本>` / `--clear-isolated <版本>`

数据隔离的两个子操作。复用 `versions::copy_shared_to_isolated` / `clear_isolated`。

**成本**：小。**收益**：低（同 `--shortcut`）。

---

### ⚪ P3：`--run <版本> [--detach]`

无头启动 dsh web。

⚠️ **风险高**：
- 与 GUI 的进程管理（`ProcMap` / `procreg` / Job Object）耦合
- CLI 是**一次性进程**，启动后要"留守"还是"detach"？
- 若 detach，谁来管生命周期？（这正是 `procreg` 的场景）
- 与「内嵌窗口」的互斥逻辑

**建议**：**暂不做**，除非有明确需求。

---

### ❌ 不建议：子命令风格（`dsh-multiver list` / `install X`）

**现状**：`--flag` 风格（`--list` / `--install X`）。

**子命令风格**（git/npm 式）更"现代"，但：
- **破坏兼容**：所有现有脚本要改
- 与 `--launch-version` / `--debug-progress` 等内部参数风格不一致
- 收益有限（`--flag` 风格对单层命令完全够用）

**建议**：**除非发大版本（v0.3.0）并明确接受破坏，否则不做**。

---

## 三、推荐实施顺序

| 批次 | 内容 | 成本 | 依赖 |
| :--- | :--- | :--- | :--- |
| **第 1 批** | `--json` + `--which` | ~1 天 | 无 |
| **第 2 批** | `--info` + `--versions` + `--env` | ~1.5 天 | 第 1 批（`--json` 复用） |
| **第 3 批** | `--isolate` + `--clean` | ~1 天 | 无 |
| **第 4 批** | `--doctor` | ~1 天 | 第 2 批（复用 env） |
| **可选** | `--shortcut` / `--copy-shared` / `--clear-isolated` | ~半天 | 无 |
| **暂缓** | `--run` / 子命令风格 | — | — |

**总计约 4.5~5 天**（不含暂缓项）。

---

## 四、设计约束（必须遵守）

1. **不破坏兼容**：非 `--json` 时输出**完全不变**（现有脚本不受影响）
2. **CLI 保持中文**（既定决策）：`--json` 是唯一"机读"出口，语言不影响它
3. **退出码语义**：成功 0 / 失败 1；`--env` 的"致命项失败"也算 1
4. **stdout 纯净**：进度/提示走 stderr；`--json` 时 stdout **只有 JSON**（无任何其他输出）
5. **`--dry-run` 一致性**：新增的破坏性命令（`--isolate` 等）都要支持 `--dry-run`
6. **帮助同步**：`--help` + `docs/使用手册.md` 的 CLI 章节都要更新

---

## 五、登记建议

按项目惯例，本规划应拆成 Backlog 条目记入 `docs/开发缺口.md` 的「长期 Backlog」：

| 条目 | 说明 |
| :--- | :--- |
| CLI `--json` | 结构化输出（P0） |
| CLI `--which` | 输出默认版本入口路径（P0） |
| CLI `--info` / `--versions` / `--env` | 查询类（P1） |
| CLI `--isolate` / `--clean` | 运维类（P2） |
| CLI `--doctor` | 诊断类（P2） |
| CLI 子命令风格 | ❌ 明确不做（除非大版本） |
| CLI `--run` | ⚪ 暂缓（与进程管理耦合） |

**重新考虑的条件**：有脚本化/CI 需求，或用户反馈"CLI 不够用"时，按上表推进。
