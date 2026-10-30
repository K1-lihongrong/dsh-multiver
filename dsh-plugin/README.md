# dsh-multiver-bridge

dsh-multiver 的双向桥插件：在 dsh 设置页查看已安装版本、切换默认版本、查看运行状态与一致性校对。

## 状态

设置页「版本管理」提供：状态条（默认版本 / 运行版本 + 置信度 / 待重启提示 / 一致性校对详情）、版本表、「设为默认」、四个送回动作（打开 multiver 目录 / 打开版本目录 / 打开 multiver 路径设置 / 复制诊断），以及「打开 multiver」按钮（仅非内嵌模式显示）。

首次使用时若 multiver 尚未配置数据根，页面会显示三步部署向导（选目录 → 获取 exe → 启动），下载过程有进度条与取消按钮。

底部「multiver 更新」区块可检查并更新 multiver 自身；内嵌窗口下更新会自动降级为「暂存」，下次启动 multiver 时生效，不打断当前会话。

## 安装

已验证 dsh 版本：`0.1.5-rc.3`、`0.2.0-rc.1`、`0.2.0-rc.2`、`0.2.1-rc.2`、`0.2.1-rc.3`（最低支持 `0.1.5-rc.3`）。安装层**不设版本约束**，其他 dsh 版本也能装，但页面会提示未验证。**装完必须重启 dsh 才生效。**

### 方式一：npm（推荐）

```powershell
dsh plugin --profile web add dsh-multiver-bridge
```

### 方式二：GitHub Release tarball

```powershell
dsh plugin --profile web add https://github.com/K1-lihongrong/dsh-multiver/releases/download/v0.3.2/dsh-multiver-bridge-0.2.1.tgz
```

### 方式三：本地 tarball

```powershell
cd dsh-plugin
pnpm pack
# 产物：dsh-multiver-bridge-0.2.1.tgz
dsh plugin --profile web add .\dsh-multiver-bridge-0.2.1.tgz
```

### 方式四：目录安装（仅开发用）

```powershell
.\install.ps1 -DshCmd '<dsh.cmd 路径>' -Profile mvtest
```

> 目录安装是 link，移动源码目录会失效。交付请用 tarball 或 npm。

### 辅助脚本 `install.ps1`（可选，仅 tarball/目录安装需要）

```powershell
.\install.ps1 -DshCmd '<dsh.cmd 路径>' -Profile web -PackageDir '.\dsh-multiver-bridge-0.2.1.tgz'
```

脚本行为：

- **profile 不存在** → 用 `--from-default-profile web` 建新 profile（含 `dsh-base` + `dsh-web-app`），再装插件。
- **profile 已存在但缺 `dsh-web-app`** → 默认报错退出，加 `-Force` 才降级为警告。

> ⚠️ `--from-default-profile` 的语义是**重建**（把旧目录 rename 走、放入官方模板新目录），**会清空已装 bundle**。它只能用于「创建全新 profile」，不能当成「补 web-app」的补丁。

## 卸载

```powershell
dsh plugin --profile web remove dsh-multiver-bridge
```

或使用辅助脚本（等价，会先检查 profile）：

```powershell
.\uninstall.ps1 -DshCmd '<dsh.cmd 路径>' -Profile web
```

重启后 `dsh --profile <name> --dump-config` 不应再出现 `multiver-bridge`。

## 配置

`cordis.patch.yml` 中两个字段：

| 字段 | 默认 | 说明 |
|:---|:---|:---|
| `exe` | `""` | dsh-multiver.exe 绝对路径；空 = 自动探测（优先 `DSH_MULTIVER_EXE`） |
| `commandTimeoutMs` | `15000` | 单次 CLI 调用超时 |

## 运行来源识别

multiver 启动 dsh 时会注入以下环境变量，插件据此判断「谁启动的、哪个版本」：

| 变量 | 值 | 注入方 |
|:---|:---|:---|
| `DSH_MULTIVER_VERSION` | 版本号 | 内嵌窗口 / 控制台 / 终端 `dsh` |
| `DSH_MULTIVER_ROOT` | 数据根 | 同上 |
| `DSH_MULTIVER_EXE` | 管理器 exe 路径 | 同上 |
| `DSH_MULTIVER_MODE` | `embedded` / `console` | 仅 multiver 启动（终端无） |
| `DSH_MULTIVER_PID` | multiver 进程 PID | 仅 multiver 启动（终端无） |

终端 `dsh` 不注入 MODE/PID——那里没有 launcher 进程，PID 会误导判定。

## 已知限制

- **安装层零版本约束**（不声明 `engines.dsh`），任何 dsh 版本都能装；已验证版本记在 `package.json` 的 `dshMultiver.verifiedDshVersions`（`0.1.5-rc.3` / `0.2.0-rc.1` / `0.2.0-rc.2` / `0.2.1-rc.2` / `0.2.1-rc.3`）。运行版本不在列表时，页面顶部显示黄色提示但不阻断功能。完整矩阵与测试方法见 [插件兼容矩阵](../docs/插件兼容矩阵.md)。
- 卸载后 `node_modules` 可能残留 link（dsh 的已知行为，不影响加载）。
- 「运行中」徽标只统计 multiver 打开的内嵌窗口；console 启动与终端 `dsh` 没有进程内记录，不在范围。
- 切换版本只改默认版本，**对下一次启动生效**；运行中的 dsh 无法热替换自身。
- 内嵌窗口里隐藏「打开 multiver」按钮——点它只会再开一个管理器，无意义。

## tarball 内容

`pnpm pack` 产物包含：`package.json`、`cordis.patch.yml`、`index.js`、`client.js`、`locale/*.json`、`icon.svg`、`README.md`、`install.ps1`、`uninstall.ps1`、`LICENSE`。

`*.tgz` 已加入 `.gitignore`，随 Release 分发，不进仓库。

## 发布信息

| 项 | 值 |
|:---|:---|
| 包名 | `dsh-multiver-bridge` |
| 版本 | `0.2.1` |
| npm | https://www.npmjs.com/package/dsh-multiver-bridge |
| Release | [v0.3.2](https://github.com/K1-lihongrong/dsh-multiver/releases/tag/v0.3.2) |

两条通道分发同一份 tarball。

**完整性校验**：npm 通道由 registry 的 `integrity` 字段保证；Release 通道请对照该 Release 附带的 `SHA256SUMS.txt`（本文件会随内容变化，故不在包内硬编码指纹）：

```powershell
Get-FileHash dsh-multiver-bridge-0.2.1.tgz -Algorithm SHA256
# 与 Release 页面 SHA256SUMS.txt 中同名行比对
```
