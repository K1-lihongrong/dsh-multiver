## DSH 版本管理器 v0.2.4

### 🐛 重要修复：Linux 下内嵌窗口「重新连接中」

**现象**：Linux 上点「运行」打开内嵌窗口后，页面能加载但底部常驻「重新连接中…」，所有功能不可用。

**根因**：`jobobj.rs` 在 Unix 侧给 dsh 子进程设置了 `PR_SET_PDEATHSIG=SIGKILL`（本意是"管理器退出时自动清理子进程"）。但这个信号绑定的是**发起进程创建的线程**，而本项目在 Tauri 的线程池线程里创建 dsh —— 命令一返回，池线程被回收，内核立刻按该信号把 dsh 杀掉了。

**影响范围**：所有 Linux 环境（不只是 WSLg）。Windows 使用 Job Object 机制，不受影响。

**修复**：Unix 侧移除 `PR_SET_PDEATHSIG`，只保留进程组设置（用于正常清理）。

### ✨ 新增：残留进程兜底清理

移除 `PDEATHSIG` 后，"管理器被强制杀死"时来不及清理 dsh。为此新增 `procreg.rs`：

- dsh 启动后，把它的进程组 ID 登记到 `<数据根>/logs/dsh-procs.json`
- 管理器下次启动时，清理仍存活的残留进程
- 正常关窗 / 退出仍即时清理，登记随之注销

| 场景 | 清理方式 |
| :--- | :--- |
| 正常关窗 / 退出 | 立即（`killpg`） |
| 管理器被强杀 | 下次启动时清理 |

### 🧪 测试

新增对照实验回归测试，直接证明根因与修复：

- 无 `PDEATHSIG` → 子进程**存活** ✅
- 有 `PDEATHSIG` → 子进程**被杀** ✅

测试数：Windows 63 / Linux 64，全部通过。

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
