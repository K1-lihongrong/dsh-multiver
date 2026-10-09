## DSH 版本管理器 v0.2.6

### ✨ 新增：复用官方 dsh 配置目录（~/.dsh）

从"裸装 dsh"迁过来时，你原有的会话 / 凭据 / 插件**立即可用**。

- 路径设置新增开关「复用官方 dsh 配置目录（~/.dsh）」，**默认关闭**
- 开启后，非隔离版本的 `DSH_HOME` 指向官方默认目录
  （Windows `%USERPROFILE%\.dsh`，macOS/Linux `~/.dsh`）
- **隔离版本优先级更高**（开了隔离的版本永远用自己的目录）
- 切换后**自动重生成终端转发脚本**，GUI 与终端 `dsh` 行为一致
- 首次开启有确认提示（明确"不会删数据、随时可关"）

> 详见 [docs/DSH_HOME编排方案.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/DSH_HOME编排方案.md)。

### ✨ 新增：更新提醒

启动后后台检查新版本，有则顶部显示横幅：

- 一键**下载更新**（exe 直链）
- 一键**查看更新说明**
- 可关闭；网络失败静默，不打扰

> 本次只做"提醒"，**不含**自动下载安装（后续评估）。

### ✨ 新增：CLI 功能增强 4 批全部完成

| 命令 | 用途 |
| :--- | :--- |
| `--info <版本>` | 查看单个版本详情（路径 / 默认 / 隔离 / 配置目录 / 占用） |
| `--versions` | 列出远端可用版本（从 npm 查询） |
| `--env` | 环境检查（有致命项失败时退出码 1） |
| `--isolate <版本> [--off]` | 无头开关数据隔离 |
| `--clean` | 清理孤立缓存 + 清空回收站 |
| `--doctor` | 一键诊断（目录 / 失败标记 / 已安装 / 环境） |

以上命令均支持 `--json`；`--isolate` / `--clean` 另支持 `--dry-run`。

### 说明

- **无破坏性变更**：CLI 默认输出、界面默认行为均不变
- 新增依赖：`reqwest`（仅用于更新检查）

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
