# Linux 适配开发计划

> **✅ 状态：已完成（2026-10-03）** —— Linux 已在 v0.2.0 发布（AppImage + deb）。
> 本文件保留为**历史规划档案**，记录当初的设计与踩坑；其中的"待做/计划"表述均已完成，勿按它作业。
> 当前 Linux 使用说明见 [使用手册 · Linux 支持](使用手册.md#十三linux-支持)。

> 目标：让 dsh-multiver 在 Linux 上原生运行（不依赖 Wine / 不做远程）。
> 编写日期：2026-09-30 · 基线版本：v0.1.8
> 本文档**进 git**（`docs/`），因为接手环境在 Linux，读不到仓库外的 `.cuckooCode/`。

---

## 〇、接手须知（先读这段）

### 本项目的两处"隐形文档"不在仓库里

原开发者把 AI 协作文档放在**仓库外的** `.cuckooCode/` 目录。Linux 上 clone 拿不到。若需要：

- 向原开发者索取 `.cuckooCode/` 整个目录
- 或至少索取 `CUCKOO.md`（项目总览）、`交接-v0.1.8.md`（最新交接 + 踩坑）、`踩坑记录-*.md`

### 项目形态速览

| 项 | 值 |
| :--- | :--- |
| 是什么 | DeepSeek Harness（dsh）多版本管理器 |
| 技术栈 | Tauri 2（Rust）+ Vite + Vue 3 单文件 `App.vue` |
| 后端代码 | `src-tauri/src/`：`lib.rs`（命令注册/窗口）、`versions.rs`（安装卸载）、`launcher.rs`（启动 dsh）、`actions.rs`（pnpm/脚本/打开）、`envcheck.rs`、`maintenance.rs`、`config.rs`、`jobobj.rs`（进程管理）、`cli.rs`（无头 CLI） |
| 前端 | `src/App.vue`（**无任何 Windows 硬编码**，已确认） |
| 数据根 | 默认 exe 同目录，含 `versions/` `home/` `store/` `cache/` `state/` `webview/` `logs/` |

### 核心机制（改 Linux 前必须理解）

1. **每版本独立装** `versions/<版本>/`，用 `pnpm add @deepseek-ai/dsh@<版本>` + `--config.store-dir=<根>/store`
2. **必须 `node-linker=hoisted`**（写进每版本的 `.npmrc`）—— dsh 按 Node 常规方式解析依赖，pnpm 默认符号链接结构会报 `Cannot find package`
3. **必须 `--config.dangerouslyAllowAllBuilds=true`** —— pnpm 10+ 默认不跑依赖构建脚本，node-pty/koffi 等原生模块会缺失
4. **`DSH_HOME` 决定数据目录**：非隔离版本 → `<根>/home`（共享）；隔离版本 → `<根>/versions/<版本>/home`
5. **启动 dsh**：`dsh web --port 0 --no-open` → 从 stdout 解析形如 `http://127.0.0.1:<port>/?token=...` 的完整 URL
6. **每个版本独立 WebView2 数据目录**（Windows 特有）→ Linux 对应 WebKit 的 data dir

---

## 一、现状扫描：Windows 专有代码全清单

以下是**逐行扫过的**结果，按改动难度分级：🟢 简单 / 🟡 中等 / 🔴 需重新设计 / ✅ 已有 Linux 分支

| # | 位置 | Windows 实现 | 用途 | 难度 |
| :-- | :--- | :--- | :--- | :--- |
| 1 | `jobobj.rs`（全文） | Job Object `KILL_ON_JOB_CLOSE` | 管理器退出即杀光 dsh 进程 | 🔴 |
| 2 | `launcher.rs:23,310` | `.bin/dsh.cmd` | dsh 入口 | 🟢 |
| 3 | `launcher.rs:28-35,315-322` | `cmd /C <bin>` | 执行 .cmd | 🟢 |
| 4 | `launcher.rs:48-53` | `CREATE_NO_WINDOW` | 隐藏控制台 | 🟢 |
| 5 | `launcher.rs:131-147` | `taskkill /PID x /T /F` | 杀进程树 | 🟡 |
| 6 | `launcher.rs:169-226` | PowerShell COM 生成 `.lnk` | 桌面快捷方式 | 🟡 |
| 7 | `launcher.rs:228-291` | `reg query` 读桌面路径 | 定位桌面 | 🟡 |
| 8 | `launcher.rs:293-344` | `CREATE_NEW_CONSOLE` | 新终端窗口 | 🟡 |
| 9 | `actions.rs:4-13` | `cmd /C pnpm` | 调 pnpm | 🟢 |
| 10 | `actions.rs:65-107` | 生成 **.cmd 批处理**（无 cfg 分支！） | 终端短命令 `dsh` | 🟡 |
| 11 | `actions.rs:110-115` | 写 `dsh.cmd` | 同上 | 🟢 |
| 12 | `actions.rs:117-137` | `cmd /C start` | 打开 URL | ✅ 已有 `xdg-open` |
| 13 | `actions.rs:139-150` | `explorer` | 打开文件夹 | ✅ 已有 `xdg-open` |
| 14 | `versions.rs:162-177` | `cmd /C pnpm` | 调 pnpm | 🟢 |
| 15 | `versions.rs:577-605` | `file_nlink` 读硬链接数 | 占用统计 | ✅ 已有 `unix` 分支 |
| 16 | `versions.rs:687-739` | `mklink /J` | 建 junction | ✅ 已有 `symlink` 分支 |
| 17 | `envcheck.rs:5-19` | `cmd /C` | 执行命令 | 🟢 |
| 18 | `lib.rs:252-258` | `%APPDATA%\npm` | 转发脚本目录 | 🟡 |
| 19 | `lib.rs:865-908` | AUMID + 注册表 IconUri | 任务栏图标 | 🟢 cfg 掉 |
| 20 | `cli.rs:150-193` | `AttachConsole` | GUI 程序接父控制台 | 🟢 cfg 掉 |
| 21 | `cli.rs:198-217` | `GetConsoleMode` 判 TTY | CLI 进度显示 | 🟡 |
| 22 | `maintenance.rs:52-62` | `IDLE_PRIORITY_CLASS` | store prune 低优先级 | 🟡 |
| 23 | `main.rs:2` | `windows_subsystem` | 无控制台窗口 | ✅ `cfg_attr` 已条件化 |
| 24 | `lib.rs:247` | 硬编码 `dsh.cmd` | 删转发脚本 | 🟢 |
| 25 | `launcher.rs:214` | AUMID 写进 .lnk | 任务栏分组 | 🟢 随 #6 |
| 26 | `.github/workflows/release.yml:22` | `runs-on: windows-latest` | CI | 🟡 |
| 27 | `src-tauri/Cargo.toml:29-37` | `windows-sys` | Win API | ✅ 已 `[target.'cfg(windows)']` |

**关键发现**：

- ✅ 前端 `App.vue` **零 Windows 痕迹**，不用动
- ✅ `Cargo.toml` 的 `windows-sys` 已在 target 条件下，Linux 不会拉取
- ⚠️ `actions.rs` 的 `build_forward_script` **完全没有 cfg 分支**，Linux 下会生成无用的 .cmd（表格 #10）
- ⚠️ 现有 `#[cfg(not(windows))]` 分支大多是**照抄的 fallback，从未在真 Linux 上验证**

---

## 二、分阶段计划

建议按 6 个 PR 递进，每个 PR 结束时程序都**能编译 + 有可验证的增量**。

### 阶段 0：Linux 开发环境就绪（不写代码）

**产出**：能在 Linux 上 `pnpm tauri dev` 起来，看到主界面（功能可以坏）。

~~~bash
# 1. 系统依赖（Ubuntu/Debian 系）
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  build-essential curl wget file \
  libxdo-dev libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev

# 2. Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 3. Node 22 + pnpm 11
#    （Node 用 nvm/fnm 均可；pnpm 用 corepack 或 npm i -g pnpm）

# 4. 拉代码
git clone https://github.com/K1-lihongrong/dsh-multiver.git
cd dsh-multiver
pnpm install
pnpm tauri dev
~~~

**验收**：窗口起来，界面渲染正常，环境检查面板能跑（可能报 Node/pnpm 之外的错）。

**注意**：

- Ubuntu 22.04 比 24.04 更省事（webkit2gtk-4.1 版本兼容）
- 若报 `webkit2gtk not found`，检查 `pkg-config --modversion webkit2gtk-4.1`

---

### 阶段 1：编译通过（cfg 分支补全）

**目标**：`cargo check` 在 Linux 上零错误。

**任务**：

1. **`Cargo.toml` 加 Unix 依赖**

   ~~~toml
   [target.'cfg(unix)'.dependencies]
   libc = "0.2"
   ~~~

   （用于 `isatty`、`killpg`、`setpriority`）

2. **`main.rs:2`** —— `cfg_attr` 已条件化，无需改。但确认 `windows_subsystem` 在 Linux 被忽略。

3. **`lib.rs:865-908` 的 `set_windows_app_user_model_id`** —— 已有 `#[cfg(not(windows))]` 空实现，OK。

4. **`jobobj.rs`** —— 已有 `#[cfg(not(windows))]` 空实现，能编译。**功能留到阶段 4**。

5. **`actions.rs` `build_forward_script`** —— 目前**无 cfg 分支**，会生成 Windows 批处理内容。加分支（详见阶段 5）。

6. **`lib.rs:247,254` 的 `path_bin_dir`** —— `%APPDATA%\npm` 在 Linux 下拿不到，会 fallback 到管理器目录。**编译能过，功能留到阶段 5**。

7. **`cli.rs:198-217` 的 `progress_is_tty`** —— 目前 Linux 硬编码 `false`。改成：

   ~~~rust
   #[cfg(unix)]
   fn progress_is_tty() -> bool {
       unsafe { libc::isatty(libc::STDERR_FILENO) == 1 }
   }
   ~~~

8. **`maintenance.rs:52-62`** —— 已有 `#[cfg(not(windows))]` 分支（应是直接跑），确认能编译。

**验收**：`cargo check` 零警告零错误。

---

### 阶段 2：核心功能（安装 / 卸载 / 列表 / 扫描）

**目标**：能装一个 dsh 版本、能列出、能卸载、能扫占用。

**好消息**：这一层大部分已经跨平台：

- `versions.rs:162-177` 的 `pnpm_command()` 已有 `#[cfg(not(windows))] { Command::new("pnpm") }`
- `versions.rs:596-600` `file_nlink` 已有 `#[cfg(unix)]` 用 `MetadataExt::nlink()`
- `versions.rs:735-738` `create_junction` 已有 `#[cfg(not(windows))] { std::os::unix::fs::symlink(...) }`

**要验证/修的点**：

1. **pnpm 调用**：Linux 下 `Command::new("pnpm")` 依赖 PATH。若 pnpm 由 corepack 管理，可能需要绝对路径。**建议**：加一个 `which pnpm` 探测，失败时给清晰错误。

2. **`install` 的 store/cache/state 参数**（`versions.rs:401-403`）：Linux 下 `--config.store-dir=<绝对路径>` 同样有效，无需改。

3. **`create_junction` → symlink 的语义差异**：
   - Windows junction 对目录有效，无需管理员
   - Linux `symlink` 对目录有效，无需特权
   - **但要确认 `copy_shared_to_isolated` 的逻辑**（`versions.rs:641+`）在 Linux 上"遇 symlink 重建"的判断正确 —— 它用 `is_reparse_point`（Linux 分支用 `file_type().is_symlink()`），逻辑对。

4. **硬链接统计**：`dir_size_detail` 遍历时 `nlink > 1` 算共享。Linux 上 pnpm 默认也建硬链接到 store，**行为一致**，无需改。

5. **`mklink` 那个坑**（分开传参）在 Linux 不适用 —— 已经是 `std::os::unix::fs::symlink`，没这问题。

**验收**：

~~~bash
cargo run -- --list
cargo run -- --install 0.2.0-rc.2
cargo run -- --list          # 应看到新版本
cargo run -- --uninstall 0.2.0-rc.2
~~~

---

### 阶段 3：启动 dsh（内嵌窗口 / 浏览器打开）

**目标**：点「运行」能在应用内打开 dsh Web 界面。

**要改的点**：

1. **`launcher.rs:23,310` 的入口路径**

   ~~~rust
   .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" })
   ~~~

   ✅ 已正确。但要**验证** `node_modules/.bin/dsh` 在 Linux 上存在且可执行。

2. **`launcher.rs:28-35` 的 `cmd /C`**

   ✅ 已有 `#[cfg(not(windows))] let mut cmd = Command::new(&bin);`。但要确认 `.bin/dsh` 是脚本，需要**可执行权限**（pnpm 安装时会自动 chmod，通常 OK）。

3. **`launcher.rs:48-53` 的 `CREATE_NO_WINDOW`**

   ✅ 已在 `#[cfg(windows)]` 块里。Linux 无需隐藏窗口（`spawn` 默认不产生终端）。

4. **WebView 数据目录**（`lib.rs` `launch_window` 里的 `.data_directory(...)`）：
   - Windows 是 `webview/<版本>`（WebView2）
   - Linux 是 WebKitGTK，**`data_directory` API 在 Tauri 2 的 Linux 后端支持情况需验证**
   - **风险**：若 Linux 不支持按窗口隔离数据目录，431 问题可能复现。**备选**：设 `WEBKIT_FORCE_SANDBOX=1` 或用独立 profile 环境变量

5. **User-Agent**（`lib.rs` 的 `.user_agent("Mozilla/5.0 ... Edg/120")`）：
   - 保留即可（dsh 据此判断是不是浏览器）
   - 但 Linux 上用一个 Edge UA 有点怪，可考虑改成 Chrome UA（不影响功能）

**验收**：

- 点「运行」→ 窗口打开 → 显示 dsh Web 界面（不是白屏、不是 431）
- 关闭窗口 → `ps aux | grep dsh` 确认进程被杀

---

### 阶段 4：进程管理（最重要、最容易踩坑）

**目标**：管理器退出/崩溃时，dsh 进程（含其 node 子进程树）**自动被杀**。

**Windows 的做法**：Job Object + `KILL_ON_JOB_CLOSE`（`jobobj.rs`）。这是 OS 级保障，即使管理器被强杀也生效。

**Linux 的等价方案（两层保险）**：

#### 方案 A（主）：进程组 + 显式 killpg

~~~rust
// 启动时：让子进程成为新进程组组长
use std::os::unix::process::CommandExt;
cmd.process_group(0);   // Rust 1.64+ 稳定，pgid = 子进程 pid

// 杀时：kill 整个进程组
#[cfg(unix)]
pub fn kill_tree(child: &mut Child) {
    let pid = child.id() as i32;
    unsafe { libc::killpg(pid, libc::SIGKILL); }
    let _ = child.kill();   // 兜底
}
~~~

> ⚠️ `process_group(0)` 必须在 `spawn()` 之前调用。`jobobj.rs` 的角色变成"给 launcher 用"的辅助。

#### 方案 B（兜底）：`PR_SET_PDEATHSIG`

防"管理器被 SIGKILL 强杀、来不及跑清理代码"的情况：

~~~rust
#[cfg(target_os = "linux")]
unsafe {
    use std::os::unix::process::CommandExt;
    cmd.pre_exec(|| {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        Ok(())
    });
}
~~~

> ⚠️ **坑**：`PDEATHSIG` 是"父**线程**"死亡时触发，不是父进程。多线程程序里要注意设置时机。且**只对直接子进程有效**，孙进程（node 的子进程）靠方案 A 的进程组覆盖。

#### 方案 C（可选）：systemd scope / cgroup

若想更彻底（连逃逸出进程组的孙进程也管），可以用 `systemd-run --scope`。**复杂度高，不建议一期做**。

**推荐组合**：**A（进程组）+ B（PDEATHSIG 兜底）**

**要改的文件**：

1. `jobobj.rs` → 加 `#[cfg(unix)]` 实现，导出统一的 `ProcessGuard`
2. `launcher.rs:132-147` `kill_tree` → 加 Unix 分支
3. `launcher.rs:55-60` → `assign_to_new_job` 换成跨平台的 `guard_child(&mut cmd)`（在 spawn 前配置）
4. `lib.rs` 的 `RunEvent::ExitRequested` 清理逻辑（已有）→ 确认在 Linux 上触发

**验收**：

~~~bash
# 1. 正常退出
cargo run -- --list &
# 关掉窗口
ps aux | grep -E "dsh|node" | grep -v grep   # 应为空

# 2. 强杀管理器（模拟崩溃）
kill -9 <管理器 pid>
ps aux | grep -E "dsh|node" | grep -v grep   # 应为空（PDEATHSIG 生效）
~~~

---

### 阶段 5：终端短命令 + 快捷方式 + 打开目录

**目标**：`--set-default` 后终端能敲 `dsh`；能创建桌面快捷方式；能打开目录。

#### 5.1 转发脚本 `actions.rs` `build_forward_script`

**现状**：只生成 Windows 批处理，无 cfg 分支。

**改法**：拆成两个函数，用 cfg 分派：

~~~rust
#[cfg(windows)]
pub fn build_forward_script(version: &str, root_dir: &str, isolated: bool) -> String {
    /* 现有批处理内容，不动 */
}

#[cfg(unix)]
pub fn build_forward_script(version: &str, root_dir: &str, isolated: bool) -> String {
    let root = root_dir.trim_end_matches('/');
    let home = if isolated {
        format!("{}/versions/{}/home", root, version)
    } else {
        format!("{}/home", root)
    };
    format!(
        "#!/bin/sh\n\
         # dsh-multiver 转发脚本（自动生成，请勿手改）\n\
         VER='{}'\n\
         ROOT='{}'\n\
         BIN=\"$ROOT/versions/$VER/node_modules/.bin/dsh\"\n\
         if [ ! -x \"$BIN\" ]; then\n\
         \x20 echo \"[dsh] 找不到版本 $VER 的入口: $BIN\" >&2\n\
         \x20 exit 1\n\
         fi\n\
         if [ -d '{home}' ]; then\n\
         \x20 export DSH_HOME='{home}'\n\
         else\n\
         \x20 echo \"[dsh] 数据目录不存在，回退到 dsh 默认位置: {home}\" >&2\n\
         fi\n\
         exec \"$BIN\" \"$@\"\n",
        version, root, home = home
    )
}
~~~

**`write_forward_script`**（`actions.rs:110-115`）：

~~~rust
pub fn write_forward_script(dir: &Path, content: &str) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    #[cfg(windows)]
    let path = dir.join("dsh.cmd");
    #[cfg(unix)]
    let path = dir.join("dsh");
    std::fs::write(&path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(path.to_string_lossy().to_string())
}
~~~

#### 5.2 脚本目录 `lib.rs` `path_bin_dir`

**现状**：只认 `%APPDATA%\npm`。

**改法**：Linux 下按优先级探测（**选第一个存在且可写的**）：

1. `~/.local/bin`（XDG 标准，多数发行版已在 PATH）
2. `~/.local/share/pnpm`（pnpm 默认 global bin 目录）
3. `~/.npm-global/bin`（npm 自定义 global）
4. 都不存在 → **创建 `~/.local/bin`**（并提示用户加进 PATH）

~~~rust
#[cfg(unix)]
fn path_bin_dir(manager: &PathBuf) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.local/bin", home),
        format!("{}/.local/share/pnpm", home),
        format!("{}/.npm-global/bin", home),
    ];
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.exists() { return p; }
    }
    let fallback = PathBuf::from(&candidates[0]);
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}
~~~

**同时**：`lib.rs:247` 的 `target.join("dsh.cmd")` 要改成跨平台文件名。

> ⚠️ **PATH 提示**：设完默认版本后，若 `~/.local/bin` 不在 PATH，UI 应给提示。可在 `envcheck.rs` 加一项"脚本目录是否在 PATH"。

#### 5.3 桌面快捷方式 `launcher.rs:169-291`

**现状**：PowerShell COM 生成 `.lnk`，桌面路径从注册表读。

**改法**：生成 **`.desktop` 文件**。

~~~rust
#[cfg(unix)]
pub fn create_desktop_shortcut(exe_path: &Path, version: &str) -> Result<String, String> {
    let desktop = desktop_dir().ok_or_else(|| "无法定位桌面目录".to_string())?;
    let file = desktop.join(format!("DSH {}.desktop", version));
    let work = exe_path.parent().unwrap_or(Path::new("."));
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=DSH {ver}\n\
         Comment=运行 DeepSeek Harness {ver}\n\
         Exec=\"{exe}\" --launch-version {ver}\n\
         Path={work}\n\
         Terminal=false\n\
         Categories=Development;\n",
        ver = version,
        exe = exe_path.to_string_lossy(),
        work = work.to_string_lossy()
    );
    std::fs::write(&file, content).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let _ = std::process::Command::new("gio")
        .args(["set", &file.to_string_lossy(), "metadata::trusted", "true"])
        .output();
    Ok(file.to_string_lossy().to_string())
}
~~~

**`desktop_dir()`**（`launcher.rs:233-249`）Linux 分支：

~~~rust
#[cfg(unix)]
fn desktop_dir() -> Option<PathBuf> {
    if let Ok(out) = std::process::Command::new("xdg-user-dir").arg("DESKTOP").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() && Path::new(&p).exists() {
                return Some(PathBuf::from(p));
            }
        }
    }
    let home = std::env::var("HOME").ok()?;
    let d = PathBuf::from(&home).join("Desktop");
    if d.exists() { return Some(d); }
    Some(PathBuf::from(&home).join("桌面"))
}
~~~

> ⚠️ **Wayland 注意**：`gio set metadata::trusted` 是 GNOME 特有；KDE/XFCE 不需要。失败不阻塞。

#### 5.4 "浏览器打开"新终端 `launcher.rs:293-344`

**现状**：`CREATE_NEW_CONSOLE`。

**改法**：探测终端模拟器，按优先级尝试：

~~~rust
#[cfg(unix)]
fn spawn_terminal(cmd_args: &[&str]) -> Result<(), String> {
    let candidates: &[(&str, &[&str])] = &[
        ("x-terminal-emulator", &["-e"]),
        ("gnome-terminal",      &["--"]),
        ("konsole",             &["-e"]),
        ("xfce4-terminal",      &["-e"]),
        ("alacritty",           &["-e"]),
        ("kitty",               &[]),
        ("xterm",               &["-e"]),
    ];
    for (term, sep) in candidates {
        if which(term).is_some() {
            let mut c = std::process::Command::new(term);
            c.args(*sep);
            c.args(cmd_args);
            if c.spawn().is_ok() { return Ok(()); }
        }
    }
    Err("未找到可用的终端模拟器".into())
}
~~~

> **备选**：如果探测太脆，可以**降级为"直接后台启动 + 用 xdg-open 开浏览器"**（不需要终端窗口）—— 反正 dsh 会自己打印 URL 并打开浏览器。

**`open_folder` / `open_url`**（`actions.rs:117-150`）：✅ 已有 `xdg-open` 分支，验证即可。

**验收**：

~~~bash
cargo run -- --set-default 0.2.0-rc.2
which dsh && dsh --version
ls ~/Desktop/*.desktop
cat ~/.local/bin/dsh
~~~

---

### 阶段 6：打包与 CI

#### 6.1 打包形态

| 形态 | 命令 | 优点 | 缺点 |
| :--- | :--- | :--- | :--- |
| **AppImage** | `--bundles appimage` | 最接近"便携"，单文件 | 需要 FUSE；首次运行慢 |
| **.deb** | `--bundles deb` | 标准包管理 | 只覆盖 Debian 系 |
| **.rpm** | `--bundles rpm` | 覆盖 Fedora/RHEL | |
| **tar.gz** | 自定义 | 通用 | 需手动处理依赖 |

**建议**：**AppImage 为主 + deb 为辅**。

~~~bash
pnpm tauri build --bundles appimage,deb
# 产物在 src-tauri/target/release/bundle/
~~~

**AppImage 的坑**：

- 用户需要 FUSE（Ubuntu 22.04+ 默认没装，要 `libfuse2`）
- 打包时可能报 `linuxdeploy` 下载失败 → 需科学上网或设镜像
- 图标必须是 PNG（已具备 32/128/256）

#### 6.2 CI 改造

`.github/workflows/release.yml` 现在只有 `windows-latest`。改成 matrix：

~~~yaml
jobs:
  build:
    strategy:
      matrix:
        include:
          - os: windows-latest
            artifact: dsh-multiver.exe
          - os: ubuntu-22.04
            artifact: dsh-multiver.AppImage
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: Install Linux deps
        if: runner.os == 'Linux'
        run: |
          sudo apt update
          sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
            libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
~~~

**注意**：

- `ubuntu-22.04`（不是 `ubuntu-latest`，后者可能滚到 24.04 导致 webkit 版本不匹配）
- Linux 产物名是 `dsh-multiver_<版本>_amd64.AppImage`，改名逻辑要适配
- Release 上传要同时包含 .exe 和 .AppImage

#### 6.3 图标

`tauri.conf.json` 的 icon 列表已含 PNG，Linux 够用。**但** `src-tauri/icons/` 下缺 `icon.png`（512×512）—— Tauri Linux 打包可能要求。检查并补。

---

### 阶段 7：文档与收尾

1. **README 改跨平台**：环境要求加 Linux 段；下载加 AppImage；去掉"Windows 专用"
2. **`docs/使用手册.md`**：卸载的 Linux 版本、终端短命令的 Linux 说明、Linux 系统依赖小节
3. **`CHANGELOG.md` + `RELEASE_NOTES.md`**：建议标 v0.2.0（跨平台是大版本）
4. **CLI 自测**：`cli.rs` 的 `setup_console` / `progress_is_tty` 在 Linux 验证

---

## 三、验收清单（Linux 全功能）

~~~text
# 环境
[ ] cargo check 零警告
[ ] pnpm tauri dev 能起窗口
[ ] 环境检查面板 5 项全绿

# 安装
[ ] --install 0.2.0-rc.2 成功
[ ] versions/<版本>/node_modules/.bin/dsh 存在且可执行
[ ] versions/<版本>/.npmrc 含 node-linker=hoisted
[ ] 进度条能走到 100%
[ ] --install 0.0.1-rc.1 正确失败并标记 broken

# 列表 / 扫描
[ ] --list 输出正确
[ ] 扫描占用显示「复用/独占」

# 运行
[ ] 内嵌窗口能打开 dsh Web 界面
[ ] 每个版本 WebView 数据隔离
[ ] 关闭窗口后进程被杀

# 隔离
[ ] 开隔离 → 复制共享数据 → symlink 被正确重建
[ ] 清隔离数据

# 终端短命令
[ ] --set-default 后 ~/.local/bin/dsh 生成且可执行
[ ] 终端敲 dsh --version 正常
[ ] 改默认版本后脚本内容更新
[ ] 卸载默认版本后脚本被删

# 快捷方式
[ ] ~/Desktop/DSH <版本>.desktop 生成
[ ] 双击能精简启动

# 进程管理（重点）
[ ] 正常退出：无残留 dsh/node
[ ] kill -9 管理器：无残留（PDEATHSIG）

# CLI
[ ] 7 个命令都正常
[ ] 退出码正确（成功 0 / 失败 1）
[ ] 输出可管道
[ ] 重定向到文件时进度不污染 stdout

# 打包
[ ] AppImage 能在干净 Ubuntu 上跑
[ ] deb 能安装/卸载
[ ] CI 双平台都出产物
~~~

---

## 四、风险与不确定项

| 风险 | 影响 | 缓解 |
| :--- | :--- | :--- |
| **WebKitGTK 的 data_directory 支持** | 若不支持按窗口隔离，431 问题可能复现 | 阶段 3 优先验证；备选：`WEBKIT_*` 环境变量或独立 profile |
| **AppImage 的 FUSE 依赖** | 无 `libfuse2` 的机器跑不了 | 文档写明；或额外提供 tar.gz |
| **终端模拟器探测** | 无头服务器下找不到终端 | 降级为"后台启动 + xdg-open" |
| **PDEATHSIG 的线程语义** | 多线程下父线程死会误杀 | spawn 前一刻设置；进程组兜底 |
| **pnpm 不在 PATH** | corepack/nvm 场景找不到 | 加 `which pnpm` 探测 + 清晰错误 |
| **Wayland vs X11** | .desktop trusted、终端行为不同 | 都测；失败不阻塞 |
| **中文目录名（桌面）** | 中文系统桌面是 `~/桌面` | `xdg-user-dir DESKTOP` 正确返回；做 fallback |
| **dsh 本身的 Linux 兼容性** | dsh 若依赖 Windows 专有模块，装了也跑不起来 | **阶段 2 最先实测** —— 最大未知数 |

> 🔴 **最高优先级的前置验证**：`@deepseek-ai/dsh` 在 Linux 上能不能装、能不能跑 `dsh web`。
>
> ~~~bash
> pnpm add @deepseek-ai/dsh@0.2.0-rc.2
> ./node_modules/.bin/dsh web --port 0 --no-open
> ~~~
>
> 若 dsh 本身不支持 Linux，整个计划要重新评估。

---

## 五、建议的 PR 拆分

| PR | 内容 | 可独立合并 |
| :--- | :--- | :--- |
| 1 | 阶段 1：cfg 补全，`cargo check` 过 | ✅ |
| 2 | 阶段 2：安装/卸载/列表验证 + 修 bug | ✅ |
| 3 | 阶段 4：进程管理（进程组 + PDEATHSIG） | ✅ |
| 4 | 阶段 3：启动 dsh + 内嵌窗口 | ✅ |
| 5 | 阶段 5：转发脚本 + 快捷方式 + 终端 | ✅ |
| 6 | 阶段 6：CI 双平台 + 打包 | ✅ |
| 7 | 阶段 7：文档 + 发版 v0.2.0 | ✅ |

每个 PR 都**不要破坏 Windows**（CI 应双平台跑）。

---

## 六、快速参考：Windows → Linux 对照表

| Windows | Linux |
| :--- | :--- |
| `cmd /C pnpm` | `pnpm` |
| `dsh.cmd` | `dsh`（POSIX sh） |
| `mklink /J` | `std::os::unix::fs::symlink` |
| `taskkill /T /F` | `killpg(pgid, SIGKILL)` |
| Job Object `KILL_ON_JOB_CLOSE` | 进程组 + `prctl(PR_SET_PDEATHSIG)` |
| `CREATE_NO_WINDOW` | 无需 |
| `CREATE_NEW_CONSOLE` | 探测终端模拟器 |
| `%APPDATA%\npm` | `~/.local/bin` 或 `~/.local/share/pnpm` |
| `.lnk`（PowerShell COM） | `.desktop`（chmod 755 + gio trusted） |
| `reg query` 桌面路径 | `xdg-user-dir DESKTOP` |
| `explorer` | `xdg-open` |
| `cmd /C start <url>` | `xdg-open <url>` |
| AUMID（任务栏图标） | 无需 |
| `windows_subsystem = "windows"` | 无需 |
| `AttachConsole` | 无需（天然有 stdout） |
| `GetConsoleMode` 判 TTY | `libc::isatty(2)` |
| `IDLE_PRIORITY_CLASS` | `setpriority` 或 `nice` |

---

## 七、给接手 AI 的话

1. **先验证 dsh 本身在 Linux 能跑**（第四节末尾），这是前提
2. **阶段 0 的系统依赖**先装齐，否则 `cargo check` 会因找不到 webkit2gtk 报错
3. **阶段 4（进程管理）是最容易踩坑的**，建议单独写测试
4. **别破坏 Windows**：每个 `#[cfg(unix)]` 都保留 `#[cfg(windows)]` 原实现
5. 前端 `App.vue` **不用动**（已确认零 Windows 硬编码）
6. 遇到 Tauri Linux 后端问题，查 [Tauri 官方文档 · Linux](https://tauri.app/start/prerequisites/#linux)

