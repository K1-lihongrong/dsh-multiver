# CLI 迁移指南（v0.2.x → v0.3.0）

> v0.3.0 把 CLI 从 **`--flag` 风格**改为**子命令风格**。这是**破坏性变更**：旧语法不再工作。
> 本文面向**写脚本调用 CLI 的用户**。修饰符（`--json` / `--dry-run` 等）语法不变。

---

## 一、命令对照表

| 旧（v0.2.x） | 新（v0.3.0） |
| :--- | :--- |
| `dsh-multiver --list` | `dsh-multiver list` |
| `dsh-multiver --which` | `dsh-multiver which` |
| `dsh-multiver --info <版本>` | `dsh-multiver info <版本>` |
| `dsh-multiver --versions` | `dsh-multiver versions` |
| `dsh-multiver --env` | `dsh-multiver env` |
| `dsh-multiver --doctor` | `dsh-multiver doctor` |
| `dsh-multiver --install <版本>` | `dsh-multiver install <版本>` |
| `dsh-multiver --uninstall <版本>` | `dsh-multiver uninstall <版本>` |
| `dsh-multiver --set-default <版本>` | `dsh-multiver set-default <版本>` |
| `dsh-multiver --isolate <版本> [--off]` | `dsh-multiver isolate <版本> [--off]` |
| `dsh-multiver --clean` | `dsh-multiver clean` |
| `dsh-multiver --shortcut <版本>` | `dsh-multiver shortcut <版本>` |
| `dsh-multiver --copy-shared <版本>` | `dsh-multiver copy-shared <版本>` |
| `dsh-multiver --clear-isolated <版本>` | `dsh-multiver clear-isolated <版本>` |
| `dsh-multiver --export [<文件>]` | `dsh-multiver export [<文件>]` |
| `dsh-multiver --import <文件>` | `dsh-multiver import <文件>` |
| `dsh-multiver --run <版本>` | `dsh-multiver run <版本>` |
| `dsh-multiver --setup-runtime` | `dsh-multiver setup-runtime` |
| `dsh-multiver --update` | `dsh-multiver update` |
| `dsh-multiver --maintenance [--cleanup\|--prune]` | `dsh-multiver maintenance [--cleanup\|--prune]` |
| `dsh-multiver --help` / `-h` | `dsh-multiver help` |
| `dsh-multiver --version` / `-V` | `dsh-multiver version` |

---

## 二、不变的部分

**修饰符语法不变**：

| 修饰符 | 用途 |
| :--- | :--- |
| `--json` | 结构化输出（机读 JSON） |
| `--dry-run` | 只预览不执行 |
| `--force` | 覆盖非本管理器生成的 dsh 命令（`set-default`） |
| `--registry <url>` | 指定 npm 源（`install`） |
| `--off` | 关闭隔离（`isolate`） |

**示例**：
```bash
# 旧
dsh-multiver --install 0.2.0-rc.2 --registry https://registry.npmmirror.com --dry-run --json

# 新（命令改了，修饰符不变）
dsh-multiver install 0.2.0-rc.2 --registry https://registry.npmmirror.com --dry-run --json
```

**GUI 启动**：**不带任何子命令**时，照常启动图形界面（不变）。

**内部参数**：`--launch-version` / `--debug-progress`（快捷方式 / 开发者用）**保持不变**，仍在 GUI 路径。

---

## 三、常见脚本迁移

### 取默认版本入口路径

```bash
# 旧
DSH="$(dsh-multiver --which)"
# 新
DSH="$(dsh-multiver which)"
```

### 判断某版本是否安装

```bash
# 旧
if dsh-multiver --list | grep -qx "0.2.0-rc.2"; then ... fi
# 新
if dsh-multiver list | grep -qx "0.2.0-rc.2"; then ... fi
```

### JSON 解析

```bash
# 旧
dsh-multiver --list --json | jq '.[].version'
# 新
dsh-multiver list --json | jq '.[].version'
```

### 环境检查（CI 预检）

```bash
# 旧
dsh-multiver --env || exit 1
# 新
dsh-multiver env || exit 1
```

---

## 四、一次性批量替换

**Windows（PowerShell）**：
```powershell
# 在脚本目录里全局替换（先备份！）
Get-ChildItem *.ps1,*.bat,*.cmd -Recurse | ForEach-Object {
  (Get-Content $_.FullName -Raw) -replace 'dsh-multiver --(\w[\w-]*)', 'dsh-multiver $1' |
    Set-Content $_.FullName
}
```

**Linux/macOS（bash）**：
```bash
# 先备份
grep -rl 'dsh-multiver --' . | xargs -r sed -i.bak -E 's/dsh-multiver --([a-z][a-z-]*)/dsh-multiver \1/g'
```

> ⚠️ 批量替换后**务必人工核对**（修饰符 `--json` / `--dry-run` 不应被替换）。

---

## 五、为什么要改

- **更符合现代 CLI 习惯**（git / npm / cargo 式）。
- **为子命令分组留空间**（未来 `version list` / `default set` 等）。
- 项目长期维护，**越早统一越好**。

---

## 六、遇到问题

- 用 `dsh-multiver help` 查看最新用法。
- 旧语法会**明确报错**（`未知参数：--list`），不会静默失败。

---

## 七、v0.3.1 新增命令（无需迁移）

以下命令是 v0.3.1 引入的，v0.2.x 里没有对应旧语法：

| 命令 | 说明 |
| :--- | :--- |
| `set-root <目录>` | 设置数据根目录。绝对路径，会创建子目录并重生成终端转发脚本。 |

支持 `--dry-run`（只预览）与 `--json`（机读输出，含 `root` / `previous` / `changed` / `created`）。

```bash
dsh-multiver set-root D:/DSH-data --json
# {"ok":true,"root":"D:/DSH-data","previous":"...","changed":true,"created":true}
```

> 该命令与图形界面「路径设置」里的「根目录」完全等价——两者写同一份 `config.json`。
