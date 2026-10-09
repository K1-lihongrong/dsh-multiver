# CLI 重构计划（v0.3.0 候选）

> 编写日期：2026-10-09 · 基线：v0.2.5
> 定位：**大版本（v0.3.0）才做**的 CLI 结构性改造。当前不做。
> 关联：[CLI增强规划](CLI增强规划.md)（功能增强，4 批）、[开发缺口](开发缺口.md)（长期 Backlog）

---

## 一、为什么要攒到大版本

本项目 v0.1.8 起 CLI 就是 **\`--flag\` 风格**（\`--list\` / \`--install X\`），已发布到 v0.2.5，**用户脚本可能已存在**。

下面的改造都是**破坏性变更**（改变命令行语法），按语义化版本规范**必须升大版本**。所以**攒到 v0.3.0 一起做**，而不是零散发布。

---

## 二、现状（v0.2.5）

### 命令行风格
\`\`\`bash
dsh-multiver --list
dsh-multiver --which
dsh-multiver --install <版本> [--registry <url>]
dsh-multiver --uninstall <版本>
dsh-multiver --set-default <版本>
dsh-multiver --maintenance [--cleanup|--prune]
dsh-multiver --dry-run      # 修饰符
dsh-multiver --json         # 修饰符
dsh-multiver --help / --version
\`\`\`

### 内部/开发者参数（同为 --flag 风格）
\`\`\`
--launch-version <版本>   # 桌面快捷方式用（GUI 路径）
--debug-progress          # 开发者用（GUI 路径）
\`\`\`

---

## 三、改造项

### R1. 子命令风格（核心）

**现状 → 目标**：

| 现在 | 目标 |
| :--- | :--- |
| \`--list\` | \`list\` |
| \`--which\` | \`which\` |
| \`--install <版本>\` | \`install <版本>\` |
| \`--uninstall <版本>\` | \`uninstall <版本>\` |
| \`--set-default <版本>\` | \`set-default <版本>\` |
| \`--maintenance [--cleanup\|--prune]\` | \`maintenance [--cleanup\|--prune]\` |
| \`--version\` | \`version\`（或保留 \`--version\`） |
| \`--help\` | \`help\`（或保留 \`--help\`） |

**修饰符保留 \`--\`**（它们不是命令，是选项）：
\`\`\`bash
dsh-multiver install 0.1.7 --dry-run --json
\`\`\`

**收益**：
- 更符合现代 CLI 习惯（git / npm / cargo 式）
- 为**嵌套子命令**留出空间（见 R2）

**代价**：**破坏所有现有脚本**。

### R2. 嵌套子命令（R1 的前提，也是它的主要收益）

子命令风格真正的价值在于**给"命令分组"腾出空间**。做 R1 时一并规划：

\`\`\`bash
# 假想的嵌套结构
dsh-multiver version list          # 列已安装
dsh-multiver version info <版本>   # 版本详情
dsh-multiver version install <版本>
dsh-multiver version remove <版本>

dsh-multiver default get           # 查默认版本
dsh-multiver default set <版本>
dsh-multiver default clear

dsh-multiver isolate on <版本>
dsh-multiver isolate off <版本>

dsh-multiver store prune           # 等价于 maintenance --prune
dsh-multiver store clean           # 等价于 maintenance --cleanup

dsh-multiver doctor
dsh-multiver env
\`\`\`

> **注意**：这是**选项**。也可以只做 R1（扁平子命令），不做 R2。取决于命令数量是否真的多到需要分组。

### R3. 兼容旧参数的"过渡期"（可选）

为减轻破坏，可在 v0.3.0 里**同时保留旧 \`--flag\` 语法一段时间**（标为 deprecated，打印警告但继续工作），到 v0.4.0 再移除。

**权衡**：
- ✅ 用户迁移平滑
- ❌ 代码里两套解析逻辑并存（复杂）
- ❌ 文档要同时写两套

**建议**：**v0.3.0 直接切**（给足 Release Notes 迁移说明），**不做双轨** —— 因为这个工具的用户量还不大，且 CLI 用户多是开发者（看文档就能改）。

### R4. 其它可在 v0.3.0 一并做的

| 项 | 说明 |
| :--- | :--- |
| **补全脚本** | 生成 bash / zsh / pwsh 的 tab 补全（子命令风格后更有价值） |
| **\`--verbose\` / \`-v\`** | 详细日志（现在只有开发者用的 \`--debug-progress\`） |
| **\`--quiet\` / \`-q\`** | 只输出结果，无提示语 |
| **\`--no-color\`** | 显式禁用 ANSI 颜色（为将来可能的彩色输出预留） |
| **退出码细化** | 现在只有 0/1；可考虑细分（如 2=参数错误、3=环境问题） |

---

## 四、v0.3.0 前应先完成的"功能增强"

CLI 功能增强（[CLI增强规划](CLI增强规划.md) 的 4 批）**建议在 v0.2.x 系列里先做完**，这样 v0.3.0 只做"结构性改造"，**不与功能开发混在一起**。

| 批次 | 内容 | 状态 |
| :--- | :--- | :--- |
| 第 1 批 | \`--json\` + \`--which\` | ✅ 已完成（v0.2.5） |
| 第 2 批 | \`--info\` + \`--versions\` + \`--env\` | ⚪ 未做 |
| 第 3 批 | \`--isolate\` + \`--clean\` | ⚪ 未做 |
| 第 4 批 | \`--doctor\` | ⚪ 未做 |

**做完后**，v0.3.0 的重构就是把这一整套命令**从 \`--flag\` 翻译成子命令**，边界清晰、可机械迁移。

---

## 五、迁移指南（发布时随 Release Notes 给出）

用户脚本迁移示例：

\`\`\`bash
# 旧（v0.2.x）
dsh-multiver --list
dsh-multiver --install 0.1.7
dsh-multiver --set-default 0.1.7
dsh-multiver --list --json

# 新（v0.3.0）
dsh-multiver version list
dsh-multiver version install 0.1.7
dsh-multiver default set 0.1.7
dsh-multiver version list --json      # 修饰符不变
\`\`\`

**要点**：**修饰符（\`--json\` / \`--dry-run\`）语法不变** —— 只有"命令"部分从 \`--xxx\` 变成子命令。

---

## 六、决策点（做 v0.3.0 时需拍板）

| # | 问题 | 选项 |
| :-- | :--- | :--- |
| 1 | 是否做 R2（嵌套子命令） | 做 / 只做扁平子命令 |
| 2 | 是否保留 \`--version\` / \`--help\` 旧写法 | 保留（兼容惯例） / 只认 \`version\` \`help\` |
| 3 | 是否做 R3（双轨过渡） | 建议不做（直接切） |
| 4 | 是否一并做 R4（补全 / quiet / verbose） | 挑做 |
| 5 | 内部参数（\`--launch-version\` / \`--debug-progress\`）是否也改 | 建议**不改**（它们是内部/开发者用，不是用户命令，保持 \`--flag\` 无妨） |

---

## 七、不做的事（明确）

- **不做双轨兼容**（R3 默认不采纳）—— 除非用户反馈强烈
- **不改内部参数风格**（\`--launch-version\` / \`--debug-progress\` 保持原样）—— 避免动 GUI 路径
- **不在 v0.2.x 里混入**子命令改造 —— 保持小版本"只加功能、不破坏"

---

## 八、触发条件

**什么时候做 v0.3.0？**

- CLI 命令**多到 \`--flag\` 风格显得笨重**时
- 或与 **macOS 适配**一起做（那时本就要大改，可顺手）
- 或有**足够用户**需要更好的 CLI 体验时

**在此之前**：只按 [CLI增强规划](CLI增强规划.md) 加功能，**不动命令行风格**。

---

## 九、给未来实现者的提示

1. **解析层要重写**：现在的 \`parse()\` 是"遍历参数找 \`--flag\`"，子命令风格需要"取第一个非 \`-\` 参数当命令，其余按子命令分发"
2. **修饰符解析可复用**：\`has_json()\` / \`has_dry_run()\` 逻辑不变（它们只看 \`--xxx\`）
3. **\`CliCommand\` 枚举要扩展**（若做 R2，可能要变成树形或两级）
4. **测试要覆盖**：新旧语法的解析、修饰符组合、错误提示
5. **帮助文本要重写**：\`print_help()\` 改为子命令式（\`dsh-multiver help <命令>\` 可选）
