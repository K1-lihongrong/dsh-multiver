# Linux 真机验收清单（v0.3.0）

> 目的：在**真实 Linux 桌面**验证 WSL 无法覆盖的项（窗口系统、桌面环境、打包、代理、进程清理）。
> **WSL 结果不能替代真机验收**；WSL 仅用于编译/单测/CLI 预检（见文末附录 C）。
> 适用产物：v0.3.0 的 **AppImage** / **deb**（或本地 `pnpm tauri build --bundles appimage,deb` 的产物）。
> 反馈：按 [附 B](#附-b问题上报模板) 提 Issue，注明环境矩阵 + 用例编号 + 实际结果 + 日志。

## 0. 环境记录（每台机器填一次）

| 项 | 值（示例） | 你的值 |
| :--- | :--- | :--- |
| 发行版 / 版本 | Debian 13 / Ubuntu 24.04 / Fedora 41 | |
| 内核 / 架构 | `uname -r` / `x86_64` | |
| 桌面环境 | GNOME 46 / KDE Plasma 6 / XFCE 4.18 | |
| 会话 | Wayland 或 X11（`echo $XDG_SESSION_TYPE`） | |
| 显示缩放 | 100% / 125% / 150% / 200% | |
| 显卡 / 驱动 | 核显 / NVIDIA 专有 / AMD | |
| WebKitGTK | `pkg-config --modversion webkit2gtk-4.1` | |
| glibc | `ldd --version \| head -1` | |
| 语言 / 字体 | `locale`，是否装中文字体（`fc-list :lang=zh`） | |
| Node / pnpm / git | `node -v && pnpm -v && git --version` | |
| 代理 | 无 / 环境变量 / GNOME 系统代理 / 其他 | |
| FUSE | `ldconfig -p \| grep libfuse` | |

> ⚠️ **Node 版本**：dsh 需要 `^22.19.0` 或 `>=24.2`。Debian 13 仓库的 Node 是 20.x，**不满足**；
> 请用 [NodeSource](https://github.com/nodesource/distributions) 或 nvm 安装 22.19+ / 24.2+。
> pnpm 需 **10+**（推荐 11+）。git 非致命，但安装 Git 来源插件时需要。
>
> 🔧 **从源码构建/复现还需系统依赖**（Debian/Ubuntu）：
> `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libxdo-dev libssl-dev build-essential pkg-config`
> （2026-10-22 WSL 实测发现：缺 `libdbus-1-dev` 会导致首次 `cargo check` 失败。）

## 1. 产物与安装（P0）

- [ ] **L-01 产物品鉴**：记录文件名、sha256（`sha256sum`）、文件大小；AppImage 已 `chmod +x`。
- [ ] **L-02 AppImage + FUSE**：有 `libfuse2` 时可直接运行，窗口正常出现。
- [ ] **L-03 无 libfuse2**：卸载/移走 libfuse2（或容器内）后直接运行，应给出失败提示；`--appimage-extract` 解包后 `squashfs-root/AppRun` 能启动（记录提示文案）。
- [ ] **L-04 deb 安装**：`sudo apt install ./dsh-multiver_*.deb`（让 apt 自动补依赖）；桌面菜单项存在。
- [ ] **L-05 deb 卸载**：`sudo apt remove dsh-multiver`；确认可执行/菜单项移除；用户数据是否保留符合预期。
- [ ] **L-06 WebKitGTK 缺失/过低**（容器或可回滚环境）：启动应有可理解的错误（不能只白屏/闪退）；记录报错。
- [ ] **L-07 数据根可写**：AppImage 挂在只读 squashfs 时，确认界面能识别「根目录不可写」并允许改到 `~/.local/share` 等可写目录。

## 2. 启动与桌面集成（P0）

- [ ] **L-08 冷启动**：双击/命令启动，窗口正常显示，无「重新连接中」。
- [ ] **L-09 中文渲染**：界面中文无方块/乱码；系统无中文字体时记录表现（预期可读或提示装字体）。
- [ ] **L-10 HiDPI**：125%/150%/200% 下文字与按钮不重叠、不截断。
- [ ] **L-11 单实例/重复启动**：连续启动两次，第二次进程应**立即退出**，第一个实例的窗口聚焦（v0.3.0 · M2m 起为单实例；旧的「支持多开」行为已废弃）。另需验证 `--launch-version` 在首个实例运行时会在其中打开对应版本的内嵌窗口。
- [ ] **L-12 关闭退出**：关闭主窗口后 `pgrep -af dsh-multiver` 无残留（除预期后台进程）。
- [ ] **L-13 桌面快捷方式**：对某版本点「创建桌面快捷方式」，检查 `~/Desktop`（或本地化桌面目录）下 `.desktop` 生成、可执行、双击能启动对应 dsh 版本。
- [ ] **L-14 无桌面环境**（SSH/headless）：创建快捷方式应给出可理解提示，不应崩溃。

## 3. 内嵌窗口与运行（P0，重点）

- [ ] **L-15 安装版本**：装一个 dsh 版本（Node/pnpm 满足后），进度、完成状态正常。
- [ ] **L-16 内嵌窗口（X11）**：点「运行」→ 内嵌窗口能连上 dsh Web UI，可交互；**重点回归 GAP-008**（不再显示「重新连接中」）。
- [ ] **L-17 内嵌窗口（Wayland）**：切到 Wayland 会话重复 L-16；记录是否有缩放/输入法/剪贴板异常。
- [ ] **L-18 浏览器打开**：点「浏览器打开」后 dsh 正常启动，且**用户始终能拿到访问地址**（无论系统有无可用浏览器）。参照 [交互设计约定 §2.1](交互设计约定.md#21-浏览器打开open_in_browser) 判断表现是否符合设计；**Linux 需有终端模拟器**（x-terminal-emulator/xterm 等），无浏览器时 URL/token 应出现在终端窗口；无终端模拟器应明确报错。
- [ ] **L-19 WebView 缓存隔离**：两个版本各跑一次，确认 `<数据根>/webview/<版本>` 目录独立、cookie 不串。
- [ ] **L-20 关窗清理进程树**：运行 dsh 后关闭内嵌窗口，`pgrep -af dsh` 无该版本残留。
- [ ] **L-21 强杀恢复**：`kill -9` 管理器后重启，确认上次残留的 dsh 进程被清理（进程组逻辑）。
- [ ] **L-22 多版本并存**：两个版本交替运行/关闭，互不影响；默认版本切换生效。

## 4. CLI 与终端集成（P1）

- [ ] **L-23 `env --json`**：返回 6 项（Windows）/ **7 项（Linux，含中文字体）**；字段 `name_key`/`hint_key` 正常；
      Node 不满足时提示 `apt` 类系统安装（**不应提示 setup-runtime**）；源检查应带状态/耗时且离线明确失败。
- [ ] **L-24 便携运行时门控**：`setup-runtime` 在 Linux 应明确报「仅支持 Windows」；GUI 不显示「自动准备运行时」按钮，改为提示用系统包管理器。
- [ ] **L-25 git 缺失**：临时把 `git` 移出 PATH 跑 `env --json`，git 项应为**非致命失败**并给出引导；不阻塞 Node/pnpm。
- [ ] **L-26 版本管理 CLI**：`list` / `install` / `uninstall` / `set-default` 正常，退出码符合预期。
- [ ] **L-27 终端短命令**：转发脚本生成在候选 bin 目录之一（`~/.local/bin`、`~/.local/share/pnpm`、`~/.npm-global/bin` 中第一个已存在的，属设计行为）；在新 shell（PATH 已含该目录）敲 `dsh` 命中默认版本；PATH 未含时是否提示。
- [ ] **L-28 转发脚本清理**：切换/取消默认版本后，脚本内容/删除状态正确。

## 5. 网络与代理（P1）

- [ ] **L-29 环境变量代理**：`HTTPS_PROXY=http://127.0.0.1:7890 ./dsh-multiver...` 下：npm 源检查、版本安装、更新检查均可用。
- [ ] **L-30 仅 GNOME 系统代理（无环境变量）**：记录行为。**当前已知限制**：Linux 只读代理环境变量，不读 GNOME 系统代理 → 预期失败，需用户显式设 `HTTPS_PROXY`（把实际表现补进 Issue）。
- [ ] **L-31 镜像源安装**：镜像源不可用（如子包未同步）时应给出可理解的失败提示；安装失败弹窗可**换官方源重试**，官方源能成功。
- [ ] **L-32 离线/断网**：断网启动 → 环境检查/安装失败信息可理解，不崩溃、**环境检查 ≤10s 返回明确失败**（不再误报源可达）。

## 6. 桌面环境矩阵（P1，按条件逐项）

- [ ] **L-33 GNOME**：窗口、字体、缩放、快捷方式。
- [ ] **L-34 KDE Plasma**：同上。
- [ ] **L-35 XFCE**：同上。
- [ ] **L-36 缩放矩阵**：100/125/150/200%（至少一个 DE）。
- [ ] **L-37 X11 vs Wayland**：同一 DE 两种会话对比（L-16/L-17）。

## 7. 通过标准

- **P0 全部通过**（L-01..L-22 中 P0 项；若某项因环境不具备无法测，用 N/A + 原因）。
- **P1 记录实际结果**，允许存在已知限制（如 L-30），但必须可解释、有文档。
- 任何 **崩溃、白屏、数据损坏、进程残留** 视为阻塞发布。

## 附 A：日志与诊断命令

```bash
# 数据根：默认在可执行文件同目录；AppImage 只读挂载时请在界面改成可写目录
# 日志（相对数据根）：
ls -la "<数据根>/logs/"
tail -n 200 "<数据根>/logs/app.log" 2>/dev/null
tail -n 200 "<数据根>/logs/frontend.log" 2>/dev/null

# 进程
pgrep -af 'dsh-multiver|dsh'

# AppImage 依赖
ldd squashfs-root/usr/bin/dsh-multiver | grep 'not found'

# WebKit/显示相关环境信息
echo "$XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP"
glxinfo -B 2>/dev/null | head -20

# 可选：WebKit 合成问题规避（仅诊断用）
WEBKIT_DISABLE_COMPOSITING_MODE=1 ./dsh-multiver_*.AppImage
```

## 附 B：问题上报模板

```
环境：发行版+版本 / 内核 / DE+版本 / X11或Wayland / 缩放 / 显卡驱动
产物：AppImage 或 deb + sha256 + 版本号
用例：L-XX
预期：
实际：
复现步骤：
日志/截图：<附 app.log / frontend.log / 终端输出>
```

## 附 C：WSL 已覆盖、无需真机重复

- `cargo check` 零警告、`cargo test --lib`（Linux）、`cargo clippy -D warnings`
- CLI：`env --json`、`--help`、`list`/`set-default`/`uninstall`
- 转发脚本生成/删除
- 进程组清理逻辑（WSL 内）
- AppImage/deb **可构建**（CI Ubuntu 22.04）

> 真机仍需重点验证：**内嵌窗口连接（L-16/L-17）**、AppImage FUSE、桌面集成、Wayland、字体/缩放、代理行为。

## 附 D：WSL 实测结论（2026-10-22）

实测环境：WSL2 + WSLg（**非真机**，仅供预检参考）。

- 12 项通过（L-01~L-05、L-07、L-15、L-23、L-25~L-28），GUI/桌面/代理项受 WSLg 限制未测。
- 发现并已修复：
  - **L-24**：CLI `setup-runtime` 在 Linux 未报错、错误被当成功（exit 0）→ 现已明确报「仅支持 Windows」并返回 **exit 1**（`--json` 返回 `supported:false`）。
  - **3.1**：`--help` / `--version` 报「未知参数」且提示「用 --help 查看用法」自相矛盾 → 已补回 `--help/-h`、`--version/-V` 别名。
  - **3.2**：源码构建缺 `libdbus-1-dev` → 已补入上文依赖清单。
  - **3.3**：AppImage 只读挂载时「磁盘空间 0 MB」易误解 → 文案改为提示「只读挂载或磁盘已满；AppImage 请把数据根改到可写目录」。
- 仍需真机：L-06、L-08~L-14、L-16~L-22、L-29~L-37。

## 附 E：Ubuntu 容器实测与第二轮修复（2026-10-22）

实测环境：Ubuntu 24.04 + Xvfb/Openbox + Sway headless（仍非真实桌面）。

- 20 项通过/部分通过；不通过 **L-31**（镜像滞后）、**L-32**（离线误报 + ~71s）；7 项 N/A。
- 已修复并回归通过（2026-10-22 第二轮，基线 `7f56df5`，见验收记录 §6）：L-32（8s HTTP ping，离线 ≤10s 明确失败）、L-29（错误可见）、L-09（字体提示）、L-31（换源重试）、Node 24.2 口径、L-11 多开文档；回归项 L-23（7 项）、setup-runtime、`--help`/`--version` 均通过。
- **L-18 证据已过期**：报告中的「URL 弹窗」实现基于 `7f56df5`；`b9bfc71` 已恢复控制台窗口设计，Linux 改为终端模拟器承载控制台（见 §2.1），需在最新 main 重新复验「有终端 / 无浏览器 / 无终端模拟器」三场景。
- 证据补拍（main `7f56df5`，2026-10-22）：L-31 新文案（d115fbd「镜像未同步/可先换源重试」）已复验，旧「任何 npm 源都无法获取」不再出现；截图与命令输出见验收记录 §6.3，文件名以该节清单为准。
- 仍需真桌面：L-06、L-33~L-35、AppImage/deb 安装后的 GUI、L-17 输入法/剪贴板、L-30 GNOME 代理。

## 相关文档

- [征求协助](征求协助.md) —— 需要社区实测的事项
- [开发缺口](开发缺口.md) —— 已知问题（GAP-008 等）
- [使用手册 · Linux 支持](使用手册.md#十三linux-支持)
- [问题日志查看指南](问题日志查看指南.md)

