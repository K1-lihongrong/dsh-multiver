# 更新日志

本文件记录 DSH 版本管理器（dsh-multiver）各版本的变更。
最新版本的详细说明见 [Releases](https://github.com/K1-lihongrong/dsh-multiver/releases)。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

---

## [0.3.1] - 未发布

### 插件（dsh-multiver-bridge）

- **安装层零版本约束**：删除 `engines.dsh`，dsh 不再拦截任何版本的安装。此前精确锁 `0.2.1-rc.2`，rc.3 用户安装被直接拒绝（`installation rejected`），只能逐版本手动 `dsh plugin allow-version`。
- **已验证版本登记**：新增 `package.json` 字段 `dshMultiver.verifiedDshVersions`（`0.1.5-rc.3`、`0.2.0-rc.1`、`0.2.0-rc.2`、`0.2.1-rc.2`、`0.2.1-rc.3`）。该字段不被 dsh 读取，仅作信息记录。完整测试矩阵见 [插件兼容矩阵](docs/插件兼容矩阵.md)。
- **兼容性提示**：Host 的 `state` 端点新增 `compatibility` 字段（`runningVersion` / `verifiedVersions` / `verified`）；Client 在运行版本不在已验证列表时显示黄色提示条，**不阻断功能**。
- **设置页导航图标**：dsh shell 按 id 白名单硬编码图标，第三方 section 一律拿到齿轮（与「通用设置」重复）。改为按 label 文本定位自身按钮并替换 svg 内容（三条横杠 + 右下圆点）。定位失败时静默降级为原生齿轮，不影响功能。
- **插件面板搜索框**：位置从顶部工具栏移到插件列表上方独立成行，加放大镜图标，文案改为「筛选已安装的插件…」，避免与「添加插件」按钮混淆。
- **部署引导**：未配置数据根时，插件不再直接显示版本表，而是一个三步向导——选工作目录 → 获取 exe（自动下载 / 使用已有）→ `set-root` 并启动。`state` 新增 `provisioned` 字段（读 multiver 自身 `config.json` 探测，插件不另存状态）。新增 `provision` 端点：下载固定本仓库 latest Release、强制 SHA256 校验、失败清理临时文件不留半成品、仅允许 https、120s 超时。
- **更新管理**：插件底部新增「multiver 更新」区块，可检查并更新 multiver 自身。全部委托 multiver 自带 updater（`update --dry-run --json` 查、`update --json` 执行），插件不重复实现替换逻辑。
- **暂存更新（内嵌窗口也能更新）**：内嵌模式下 `apply` 自动降级为暂存——新 exe 下载到 `<数据根>/.update/` 并校验，写 `pending.json`；multiver **下次启动时**在 `run()` 早期完成替换（重命名旧 exe → 放入新的），用户重开即新版。替换不打断任何会话。`state` 新增 `pendingUpdate` 字段，UI 显示「待安装：vX.Y.Z」。
  - **安全加固**：缺少 SHA256 时拒绝安装（不凭文件大小放行任意二进制）；当前 exe 缺失时中止并保留 `.old`；重命名或放入失败时保留暂存供下次重试；只在替换全部成功后才删标记。
  - **路径一致性**：插件与 multiver 统一用 `resolve_root` 语义解析数据根——设过 `root_dir` 的用户，暂存与安装位置一致。
  - 下载增加重定向次数上限与主机名白名单。
  - **插件侧引入单元测试**：把路径解析、URL 校验、版本探测等纯逻辑抽到 `lib/pure.js`，用 Node 内置 `node:test` 覆盖 24 条用例（`pnpm test`）。此前插件侧零自动化测试，重构无回归保护。
- **部署下载前检查磁盘空间**：剩余空间不足 100 MB 时直接返回 `disk-full`，不开始下载。
- **空版本态文案**：明确 multiver 只管理自己装在数据根 `versions/` 下的 dsh，npm / pnpm / Python wheel / 桌面版安装的 dsh 是独立的一份、不受影响。

### CLI

- **新增 `set-root <目录>`**：设置数据根目录。复用 GUI `set_root` 逻辑——校验绝对路径、拒绝 `..`、创建子目录、验证可写、保存配置、重生成终端转发脚本。
  - Unix 上切换前对**旧**数据根执行一次残留进程清理（GAP-011）：否则旧 root 下的登记不会被新 root 的启动清理触及，其中的 dsh 可能永久残留。
  - Windows 不需要该清理：`procreg` 模块本身由 `cfg(unix)` 门控，子进程由 Job Object 做 OS 级保障。
  - 支持 `--dry-run` 预览与 `--json` 机读输出。

### 启动

- **单实例转发补回加载页**：冷启动成功后主窗口会被关闭（只剩 dsh 窗口）。此时再从桌面快捷方式启动另一个版本走的是单实例转发路径——该路径此前直接建 dsh 窗口，既不重建主窗口也不发启动事件，用户看到「无加载页、黑屏数秒、窗口突然出现」。现与冷启动路径对齐：先重建主窗口承载加载页 → 建 dsh 窗口 → 就绪后关闭主窗口。
- **启动失败态可在首次渲染时恢复**：单实例路径下 `launch_window` 可能在前端挂载前就返回错误（「版本未安装」是同步检查），`launch-progress:error` 事件会丢失，加载页永远停在「正在启动」。新增 `get_launch_error` 命令，前端首次渲染时查询并直接进错误态。

### 文档
- **新增 [插件兼容矩阵](docs/插件兼容矩阵.md)**：8 个 dsh 版本的实测结果，最低支持 `0.1.5-rc.3`。含失败原因、复现步骤与测试纪律。
- 使用手册补「与其它方式装的 dsh 共存」一节：npm / pnpm / Python wheel / 桌面版四种形态的冲突情况、判断当前 `dsh` 归属的方法、DSH_HOME 共用风险。

### 验证
- **兼容矩阵**：5 个版本完全支持（0.1.5-rc.3 / 0.2.0-rc.1 / 0.2.0-rc.2 / 0.2.1-rc.2 / 0.2.1-rc.3）；2 个因 dsh 自身缺陷无法判定（0.1.5-rc.1 / 0.1.2-rc.1，报 `requires the Cordis HMR service`，无插件对照同样失败）；1 个装不上（0.1.3-alpha.2，`fs-ext` 原生模块编译失败）。
- **四端点实测**：`state` / `probe` / `open` / `setDefault`、鉴权（401/403）、405、错误码映射、argv 版本探测、设置页 UI 渲染全部正常。
- **零约束安装**：rc.3 上删除豁免后直接安装成功，`compatibility.json` 保持空对象。
- **部署引导**：路径校验 4 例、dry-run 无副作用、真实切换、`env`/`doctor`/`list` 反映新根、幂等、真实数据根 config SHA 未变；下载 7 秒 / SHA256 校验 / 临时文件零残留；7 类失败路径均返回可读错误码；UI 门控双向翻转实测。
- **更新管理**：已最新 / 有更新 / 确认取消 / 成功 / 失败 / 内嵌降级为暂存 / 旧版 CLI 错误，全部实测通过。
- **暂存更新**：`staged` 模块 7 条单测覆盖标记读写、损坏标记、校验失败、缺失 exe 等路径；端到端验证内嵌暂存 → 重启自动安装。
- **E2E 35 项通过**（新增「无插件时不显示筛选框」「筛选框位于列表上方」两条）。
- **单元测试 156 passed / 1 ignored**（新增 5 条 `set-root` 测试）。
- `cargo fmt` / `cargo clippy -D warnings` 全绿。

## [0.3.0] - 未发布

> **破坏性变更**：CLI 从 `--flag` 风格改为子命令风格。旧语法已移除，迁移见 [CLI 迁移指南](docs/CLI迁移指南-v0.3.0.md)。
>
> **验收范围**：Windows 侧已完成本机验收；Linux 侧已实现但未做完整验收，相关项后置到 v0.3.x（见 [开发缺口 · Linux 适配后置](docs/开发缺口.md#linux-适配后置v03x)）。

### 破坏性变更
- **CLI 子命令风格**：`--list` → `list`，`--install X` → `install X` 等（全部 21 个命令）。
  - 修饰符语法不变：`--json` / `--dry-run` / `--force` / `--registry <url>` / `--off`。
  - GUI 启动不变（不带子命令）。
  - 内部参数 `--launch-version` / `--debug-progress` 不变。

### 重构（技术债拆分，均保持外部路径不变）
- **拆分 `cli.rs`（1750 行）→ `cli/` 模块**：
  - `mod.rs`（入口 + 命令枚举 + 上下文）、`parse.rs`（解析）、`output.rs`（输出辅助）、`help.rs`（帮助）、`console.rs`（控制台接入）。
  - `commands/`：`version.rs`（list/which/info/versions/install/uninstall）、`default.rs`（set-default）、`store.rs`（maintenance/clean/doctor）、`env.rs`（env）、`misc.rs`（其余）。
- **拆分 `commands.rs`（1687 行）→ `commands/` 12 文件**：按域分为 launch（462）/ scan（259）/ version（197）/ state（116）/ plugins（87）/ env（79）/ config_ops（67）/ isolated（63）/ update（54）/ backup（38）/ misc（47）/ mod（149）。`lib.rs` 的 42 条命令注册表零改动。
- **拆分 `versions.rs`（1079 行）→ `versions/` 4 子模块**：install（453）/ size（242）/ list（114）/ isolated（131）/ mod（195）。
- **拆分 `runtime.rs`（533 行）→ `runtime/` 3 文件**：bundled（321，便携 Node/pnpm）、webview2（147，WebView2 检测与引导）、mod（82，共用 `RuntimeProgress` 与 `download_to`）。
- **拆分 `launcher.rs`（979 行）→ `launcher/` 3 子模块**：process（341，启动与进程）、diagnose（344，日志解析）、shortcut（318，桌面集成）。
- **抽出 `paths.rs`（137 行）**：`path_bin_dir` / `forward_script_dir` / `regenerate_forward_script` / `official_dsh_home` / `resolve_home` 等纯路径函数，与 Tauri 无关。
- 全部经 `mod.rs` 重导出，**`crate::模块::X` 路径不变，调用方零改动**。

### 修复
- **子进程输出编码（D16）**：中文 Windows 控制台代码页为 936（GBK），此前用 `String::from_utf8_lossy` 解码子进程输出，中文全部变成 U+FFFD 替换字符，错误信息不可读（如刷新版本列表报 `pnpm ����×��∫`）。
  - 新增 `console_decode::decode_console`：先 UTF-8 严格解码（ASCII/UTF-8 零开销），失败按 GBK。
  - 替换 13 处 `from_utf8_lossy` + 改写 `install.rs` 的按行读取（`BufRead::lines()` 遇 GBK 会整行丢弃）。
  - 新增依赖 `encoding_rs 0.8.42`。
- **缓存清理（D13）**：新增 `sizecache::prune`，在 `load` 后清理已卸载版本留下的死条目。
- **进度回调去重（D15）**：`cmd_setup_runtime` 里 Node 与 pnpm 两段逐字重复的进度闭包，抽成 `runtime_progress_cb`。
- **错误态切换动效（R-13 补充）**：启动失败时加载页先播放约 800ms 的「信号崩坏」干扰（抖动 + 水平条纹），再淡入故障页；此前为瞬间跳变。
- **桌面快捷方式启动失败卡住（R-13 补充）**：此前用 `--launch-version` 快捷方式启动失败时，前端收不到错误事件，加载页停在「正在启动」不动。现由后端补发 `launch-progress(stage="error")` 并携带错误文本，前端进入错误态并显示原因。
- **单实例锁（D17 / M2m）**：取消「多管理器多开」，改为单实例。重复启动时第二实例立即退出，已运行实例的窗口恢复并聚焦。
  - 版本桌面快捷方式（`--launch-version V`）在管理器已运行时**仍然有效**：不再新开管理器，而是在已运行实例里打开该版本的内嵌窗口（同版本已有窗口则聚焦）。
  - 主窗口已被 `--launch-version` 流程关闭时，再次启动会按 `tauri.conf.json` 重建主窗口。
  - CLI 不受影响：`list --json` / `set-default --dry-run --json` 等命令仍在 `tauri::Builder` 之前分流，立即返回。
  - 已知限制：锁按应用标识全局生效，同一用户下两个不同目录的副本也不能同时运行。
  - 用户可见变化：管理器不再能开第二个实例；需要对比版本时在同一个管理器内分别点「运行」。
- **用户安装的插件无法卸载（验收 #5）**：插件面板用 `isBundle()` 禁用卸载按钮，而来源徽标用 `isOfficial()` 显示「用户安装」——自相矛盾。`dsh plugin add` 安装声明了 `dsh.bundle` 的包（如 `dsh-multiver-bridge`）会写进 profile 的 `bundles`，于是被当成内置组合包锁定。
  - 改为只按 `isOfficial()` 判定：仅「随 DSH 提供」的官方组合包保持锁定，用户安装的 bundle 可卸载。
  - 后端 `source` / `is_bundle` 语义不变（其它 UI 与排序依赖它）。
  - 文案改为「DSH 随附的官方组合包不能单独卸载」，四语言同步。
  - 用户可见变化：B1 插件面板里用户自己安装的插件/组合包可以卸载；官方组合包仍锁定。
- **「浏览器打开」控制台全空白（验收 #1）**：GUI 进程（Tauri，无控制台）只设 `CREATE_NEW_CONSOLE` 时，子进程标准句柄没有被可靠绑定到新控制台，dsh 的两行输出写进空句柄，窗口存在但全空白。
  - 显式 `stdio(inherit)` + `CREATE_NEW_CONSOLE`，让新控制台初始化标准句柄。
  - 用户可见变化：「浏览器打开」弹出带 dsh 实时输出的控制台窗口（含 URL/token）。
  - 关窗即停已验证：关闭控制台窗口后 `cmd` 与 `node` 同时终止，无残留。
- **官方随附 bundle 被误判为用户安装（验收 #5b）**：官方清单只读 `OPTIONAL_BUNDLES` / `DEFAULT_PROFILE_BUNDLES` / `ON_DEMAND_BUNDLES` 三处，而 `@deepseek-ai/dsh-web-app` 只出现在 `PROFILE_TEMPLATES.web.bundles` 里，因此被当成用户安装、卸载按钮可点。
  - 清单补上 `PROFILE_TEMPLATES` 所有模板的 bundles 并去重（15 项 → 20 项）。
  - 用户可见变化：官方随附的 `dsh-web-app` 不再显示为可卸载。
- **点「插件」按钮闪黑框（验收 #5c）**：`query_official_bundles` 启动 `node` 读取官方清单时没设 `CREATE_NO_WINDOW`，GUI 进程无控制台，Windows 为 `node` 分配了可见控制台窗口。
  - 补上该 flag，与插件命令执行路径一致。
  - 用户可见变化：点「插件」按钮不再闪黑框。

### 新增（双向桥 B1）
- **dsh 插件管理面板**：已安装版本的「插件」按钮 → 展开面板，管理该版本 dsh profile 的插件。
  - 列 profile 与插件（包名 + 版本，标 bundle）；安装/卸载（调 `dsh plugin --profile <p> add/remove`）；打开 profile 目录。
  - 新增后端模块 `dsplugins.rs` + 命令 `ds_list_plugins` / `ds_plugin_action` / `ds_open_profile_dir`。
  - 4 语言文案。
- **与官方语义对齐（2026-10-21）**：
  - 标签改为「随 DSH 提供 / 已安装 + 插件 / 依赖」：从 `dsh-app-boot` 读取官方 OPTIONAL/DEFAULT/ON_DEMAND bundle，按 `dsh.bundle` 区分插件与依赖；不再用 npm scope 猜「官方/第三方」。
  - 失败诊断本地化：识别官方 CLI 的 `allow-version` / `pnpm was not found` / `diagnostics: <logPath>`。
  - 卸载二次确认、内置 bundle 禁卸、包名校验（防 pnpm 选项/特殊字符透传）。
  - 进度输出面板 + 300s 超时（**取消按官方边界挂起**，见 `docs/插件面板-取消设计.md`）。
- **官方信息查询与桥接**：
  - profile 信息：运行时版本、版本豁免（`compatibility.json`）、待决定构建脚本、诊断日志目录。
  - 版本豁免可查看/撤销（官方 CLI `revoke-version`）。
  - 待决定构建脚本一键「允许并重试」（写 `pnpm-workspace.yaml` 的 `allowBuilds: true`）。
  - 打开 `.plugin-manager/logs` 诊断日志目录。
  - 新增命令 `ds_profile_info` / `ds_exemption_action` / `ds_approve_builds` / `ds_open_plugin_logs`。
- **安装交互对齐官方**：
  - 「添加插件」弹窗：包名 / GitHub 地址 / 本地目录三类 spec、可折叠安装引导（含「填入示例」）、安装源选择（默认 pnpm / npm 官方 / 中国大陆镜像 / 自定义 URL）、信任提示。
  - 多阶段状态：表单 → 确认安装（显示 spec 与来源）→ 安装中（实时输出）→ 完成（显示已装版本 + 「安装成功不保证激活」提示）/ 失败（重试 / 更换安装源）；记住上次安装源（localStorage）。
  - 后端改为 `node bin.js` 直接执行 dsh CLI（避免 `cmd /C` 二次转义，安全支持 URL/路径 spec）；安装源 URL 校验对齐官方（http(s)、无 userinfo、无 `#`）。
- **更新检测与豁免允许**：
  - 「检查更新」：`dsh plugin --profile <p> outdated --json`，显示「有新版本」，更新按官方语义先卸载再安装。
  - 版本豁免支持「允许此版本」（官方 `allow-version --accept-risk`，带风险确认）。
- **安装前检查与安装源探测**：
  - 安装前 `inspect`：npm 包经官方 CLI `pnpm view` 读取名称/版本/描述/`dsh.bundle`；本地目录读 package.json；git 只显示主机。非组合包按官方语义拒绝。
  - 首次使用（未记住安装源）时并发探测 npm 官方源与镜像，选最快（1.5s 超时，缓存 5 分钟）。
  - 安装失败弹窗内可直接「允许 pnpm 拦截的构建脚本并重试」（对齐官方失败界面）。
- **配置健壮性（D18 / D19）**：
  - **D18**：管理器 `config.json` 损坏时，启动路径改用 `Config::load_checked`——写 `logs/app.log` 警告 + 把原文件重命名为 `config.json.broken-<时间戳>` 保留现场 + 回退默认配置。此前静默回退，用户 `isolated_versions` 标记丢失无感知。
  - **D19**：数据根不可写时，`AppState` 新增 `root_writable` / `root_writable_error`，界面顶部显示**不可关闭**横幅（含当前路径 + 「去设置」入口），根目录改好后自动消失。此前 `dirs.ensure()` 有 7 处静默忽略错误。
- **启动失败页背景（R-13）**：设计提供故障页背景（鲸鱼娘手持 SYSTEM FAULT 全息面板，降饱和 + 噪点质感），`LaunchScreen` 在 `stage === 'error'` 时切换，此前一直用正常态欢迎图。浅色/深色主题共用，1280x1280 / 275 KB。
- **包不存在文案（R-14）**：`dsplugins.inspectNotFound` 原写「所有安装源都没有这个包」，与下方「更换安装源」按钮矛盾——`ERR_PNPM_FETCH_404` 无法区分「包名打错」与「镜像滞后」。改为中性表述「当前安装源找不到该包，可更换安装源重试；若仍失败请检查包名」，4 语言同步。
- **运行时依赖加固**：
  - 便携运行时下载改为 **Windows-only**（Linux 不再误下载 win-x64 二进制；非 Windows 隐藏按钮并提示用系统包管理器）。
  - 环境检查新增 **git**（非致命，Git 来源插件需要）；pnpm 增加 **10+ 版本校验**（`allowBuilds` 依赖）。
  - HTTP 下载/更新/安装源探测统一客户端：未设置代理环境变量时**读取 Windows 系统代理**（注册表 Internet Settings）。
  - 2026-10-22 WSL 实测补漏：CLI `setup-runtime` 在非 Windows 明确报错并 **exit 1**（`--json` 返回 `supported:false`）；补回 `--help/-h`、`--version/-V` 别名；只读挂载 0 MB 文案澄清；Linux 源码构建依赖补 `libdbus-1-dev`（文档）。
- **Linux 验收第二轮修复（2026-10-22，容器实测）**：
  - **L-32**：registry 检查改为 8s 超时 HTTP `/-/ping`，不再用 `pnpm view`（其离线缓存会误报可达）；离线/死代理明确失败，耗时从 ~71s 降到 ≤8s。
  - **L-09**：Linux 环境检查新增中文字体项（`fc-list :lang=zh`，非致命），缺字体时提示安装 `fonts-noto-cjk`。
  - **D20**：Node 版本低于下限时，环境检查误报「未找到 node 命令」（与「真没装」共用 i18n 文案）。新增 `env.node.detail_low`，区分两种原因，4 语言同步。
  - **L-18**：GUI「浏览器打开」严格按交互设计约定保留控制台窗口：Windows 用新控制台 + dsh 自己开浏览器；Linux 查找系统终端模拟器承载控制台（无浏览器时 URL/token 仍可见，无终端则明确报错）。
  - **L-31**：镜像源子包缺失（`ERR_PNPM_NO_MATCHING_VERSION`）时，失败弹窗提供换官方源重试；弹窗文案改为「镜像未同步 / 可换源重试」，避免与换源入口自相矛盾。
  - **R-12**：镜像完全未发布某子包时（`ERR_PNPM_NO_VERSIONS`，此前归为 `unknown` 只弹 toast）一并归入 `incomplete-version`，与 `NO_MATCHING_VERSION` 同样触发换源重试。
  - **L-29**：更新检查失败写入 `error` 字段；GUI 显示可关闭的错误横幅，CLI 返回 `error` + exit 1，不再伪装「无更新」。
  - Node 下限口径统一为 `22.19+ / 24.2+`（24.0/24.1 缺 `import.meta.main`）。
  - L-11：文档声明支持多开（共享数据根注意事项）。

### 新增（dsh 插件）
- **新包 `dsh-plugin/`（`dsh-multiver-bridge`）**：在 dsh 设置页提供「版本管理」，Host 半走 `ctx.webServer.register` 路由 + `ctx.connection.requestRejection` 鉴权，Client 半在 `settings.section` 渲染。
  - 状态条：默认版本 / 运行版本（含 `high` / `low` / `conflict` / `unknown` 置信度徽标）/ 待重启提示 / 一致性校对详情。
  - 版本表：与 `dsh-multiver list --json` 逐字段一致；「设为默认」带内联确认条。
  - 三个送回动作：打开 multiver 目录、打开版本目录、复制诊断信息（剪贴板不可用时降级为可选中文本框）。
  - 「打开 multiver」按钮：仅非内嵌模式显示（契约 v1.2 的 `open.launch-multiver`）。
  - 中英双语；冲突态不误报「待重启」。
- **刷新策略**：页面可见时每 60 秒轮询；从隐藏恢复可见时立即刷新一次；隐藏时停 timer；`visibilitychange` 与 timer 在组件卸载时清理。
- **环境变量注入（`launcher/process.rs` + `actions.rs`）**：multiver 启动 dsh 时注入 `DSH_MULTIVER_VERSION` / `ROOT` / `EXE` / `MODE` / `PID`，供插件识别启动来源与模式。终端 `dsh` 转发脚本只注入前三项——那里没有 launcher 进程，PID 会误导判定。
- **multiver 主界面**：窗口获得焦点或从隐藏恢复可见时自动刷新（300ms 去抖），在 dsh 内切完默认版本后回到 multiver 即可看到新状态。
- **「运行中」徽标**：版本列表标出正在内嵌窗口里运行的版本（基于内存 ProcMap，5 秒轮询，仅页面可见时）。console 启动与终端 `dsh` 没有进程内记录，不在范围（已在悬停提示说明）。
- **单实例锁**：同 build 的第二个实例在插件 setup 阶段被拦下并转发 `--launch-version`；不同副本启动时弹原生对话框提示两个路径、聚焦已有窗口、写日志。对话框改为非阻塞 `.show()`，避免阻塞插件事件窗口过程。
- **插件打包与安装**：`pnpm pack` 产出 tarball；`install.ps1` / `uninstall.ps1` 存为 UTF-8 with BOM 并检查 `$LASTEXITCODE`；缺 `dsh-web-app` 时默认报错，`-Force` 才降级。

### 文档
- 新增 [CLI 迁移指南](docs/CLI迁移指南-v0.3.0.md)（旧→新对照 + 脚本迁移示例）。
- 使用手册与 README 同步更新。

### 新增
- **启动加载页**：快捷方式启动 / 运行按钮时立即显示加载页。
  - 设计稿视觉：全屏背景（WebP 优化，PNG 14.6MB → 167KB，-99%）+ 玻璃面板 + DSH 鲸鱼 logo + 版本号 + 光点脉冲 + 流光条。
  - 首帧即启动态（main.js 挂载前查启动模式，作为初始 prop），不闪主界面。
  - 失败时显示错误 + 「返回」/「查看日志」。
  - 浅/深色跟随主题；4 语言。
- **运行按钮已开则聚焦**：同版本窗口已开时不重启，直接聚焦（新增 is_version_open 命令）。
- **共享 home 确认对话框**：改为自定义对话框，含「我知道了（以后不再提示）」。
- **E2E 测试**：引入 Playwright（11 个测试：渲染/启动态/对话框/i18n）。

### 优化
- **启动性能**：首屏 900ms → 25~51ms。
  - 挂载前查启动模式（顶层 await）。
  - listen 后台注册（不阻塞）。
  - 启动模式跳过 refresh。
  - 窗口不再 hide→show（消除 WebView2 恢复渲染 ~900ms）。
- LAUNCH_MODE 提前到 run() 开头记录（先于窗口/前端创建）。
- 新增 show_main_window / get_launch_mode / is_version_open 命令。

### 调研结论（无代码改动）
- **P7 占用扫描「思路 G」实施后证伪**：把「`metadata()` 取大小 + `open` 取 nlink」合并为一次 open，
  实测 release 构建 **39.13s vs 旧 38.16s（同版本同机）**——瓶颈在 `File::open` 本身，合并系统调用无收益。
  代码已回退，结论记入 [开发缺口 · P7](docs/开发缺口.md)。A~G 全部验证完毕，P7 结案。

---

## [0.2.13] - 2026-10-15

### 新增
- **占用扫描体验优化**：扫描需遍历版本目录（约 26000 文件）逐个读硬链接数，约 36s/版本。本次不改算法，改交互。
  - **两段式**：第一段只读目录项大小（1~3s）立即显示总量 + "正在统计…"不确定进度条；第二段后台逐文件读硬链接数，回填「复用/独占」+ 文件级百分比。
  - **进度反馈**：扫描全部显示顶部两条进度条（版本级 + 文件级）；单版本只在行内显示文件级进度。
  - **动效**：当前行呼吸灯、完成行闪烁、待扫行浅灰（排队感，禁用其卸载）。
  - **可取消**：扫描中按钮变「取消扫描」，点击立即中断当前版本（毫秒级）；被中断版本丢弃，已扫完保留。
  - **结果缓存（版本级）**：`<根>/cache/size-cache.json`（约 200B/版本）；每版本单独记指纹（版本目录时间 + store 目录时间）；store 未变时装/卸不牵连其他版本；命中版本逐个出现并显示"已复用缓存"标签（扫描结束渐隐）。
  - **性能**：第一段收集文件列表供第二段复用，省一次目录遍历；取消标志传入递归每层，即时生效。
  - **GUI**：「扫描全部占用」按钮（工具栏）；单版本扫描保留。
  - **CLI**：`--info` 走版本级缓存，命中秒开。

---

## [0.2.12] - 2026-10-14

### 清理（无功能变更）
- **清 10 条 Clippy 警告**：collapsible_if / doc_lazy_continuation×5 / ptr_arg / derivable_impls / io_other_error / unnecessary_cast。
- **全量 `cargo fmt`**：单独一个纯格式化提交，并在仓库加 `.git-blame-ignore-revs`。
- **CI 加 lint 门禁**：`ci.yml` 增 `cargo clippy -- -D warnings` + `cargo fmt --check`。
- **`useI18n.js` 拆分**：958 → 91 行；4 语言词条拆到 `src/composables/i18n/*.js`（词条逐条核对一致，4 语言渲染验证通过）。

### 增强
- **故障报错解析增强（GAP-015）**：dsh 启动失败时，识别已知故障模式并在错误提示末尾追加「诊断建议」。共 **11 类**：
  - 通用 6 类：profile bundle 缺失（共享 home 不兼容）/ Node 版本过低 / 端口占用 / 网络不通 / 磁盘满 / 权限不足
  - dsh 专用 5 类（源码调研）：profile 已存在 / desktop profile 由 Electron 独占 / 插件版本不兼容 / git 插件 build 被 pnpm 拦截 / 缺 profile 参数
  - 纯函数 `launcher::diagnose`；诊断用**完整**错误文本（避免关键词被末尾行截断）；单测覆盖各模式 + 边界。

### 修复
- **日文界面路径显示成 ¥**：路径类文本（顶栏根目录、路径设置各目录、根目录输入框）改用等宽字体，
  避免日文字体（Meiryo 等）把反斜杠 U+005C 渲染成 ¥。非内容错误，是字体字形差异。

---

## [0.2.11] - 2026-10-14

### 优化
- **环境检查并行化**：`run_all` 的 5 项检查（node/pnpm/根目录可写/磁盘/npm 源连通）改为
  scoped threads 并行，总耗时取最慢一项。实测（各 5 次取中位数）`--env` **6373ms → 4384ms**（↓31%）。
- **版本占用扫描（P7）**：尝试并行化失败（磁盘 IO 密集，多线程反而更慢），已回退，保留原状。

---

## [0.2.10] - 2026-10-14

### 新增
- **一键自动更新（C2）**：从"提醒 + 手动下载"升级到"一键更新"。
  - 界面：有新版时更新横幅出现「一键更新」按钮 → 下载 → 校验 → 替换自身 → 重启
  - CLI：`dsh-multiver --update`（`--dry-run` 只检查）
  - **SHA256 校验**：用 Release 资产的 digest 校验下载的 exe，不通过则中止
  - **失败不损坏**：替换用辅助批处理（等主进程退出 → 覆盖 → 重启）；下载/校验失败时原程序不受影响
  - 仅 Windows 支持自动替换（Linux 走包管理器）

---

## [0.2.9] - 2026-10-14

### 新增
- **运行时适配（便携 Node / pnpm 自动准备）**：检测到系统缺 Node/pnpm 时，按需下载
  便携运行时到数据根 `<root>/tools/`，并在装/跑 dsh 时注入子进程 PATH。
  - exe 体积不变（约 9MB），不内置、不偷偷下、不污染系统
  - 环境检查联动：系统缺但便携就绪 → 显示「将使用便携」并通过
  - 界面「自动准备运行时」按钮（带下载进度条）；CLI `--setup-runtime`（`--dry-run`/`--json`）
  - 便携 Node 22.19.0 / 便携 pnpm（@pnpm/exe 独立版）
- **WebView2 早期检测引导**：缺 WebView2 时（管理器起不来），在 Tauri 初始化前用
  Win32 原生弹窗引导——[自动下载并安装 Bootstrapper] / [打开微软下载页]。
- **便携版提示横幅 + 正式安装引导对话框**：说明便携版装在数据根、想全局安装可去官网。

### 修复
- **npm 源连通检查回退便携 pnpm**：系统无 pnpm 但便携就绪时，此前本项误报失败。

---

## [0.2.8] - 2026-10-09

### 新增
- **CLI `--import <文件>`**：导入 `--export` 生成的备份文件（`--dry-run` 预览将改动的字段）。
- **界面「导入配置」按钮**：选备份文件 → 预览差异 → 二次确认 → 应用。
  与「导出配置」配套，完成换机迁移闭环。（补 GAP-013）
- **修复 dsh 命令冲突**（GAP-014）：
  - 管理器生成的转发脚本带标记 `dsh-multiver-generated`
  - 「设为默认」时若目标位置已有**非本管理器**的 `dsh` 命令 → 提示确认（GUI）/ 拒绝并要求 `--force`（CLI）
  - 卸载时**只删本管理器生成的**转发脚本，不再误删官方 `npm i -g` 装的 dsh
- **CLI `--run <版本>`**：启动 dsh web（新控制台窗口 + 自动打开系统浏览器）。
  与 GUI 的「浏览器打开」行为一致——**关掉控制台窗口即停止服务**。
- **环境检查支持完整多语言**：检查项名、`detail`（结果描述，如"满足 22.19+ / 24+"、"可用 X GB"）、
  `install_hint`（未通过时的引导说明）现在都随界面语言切换（此前硬编码中文）。

### 修复
- **导出按钮移到不依赖数据加载的位置**：此前在 `v-if="state"` 的维护面板内，
  极端情况下（数据未加载）不显示；现移到路径设置区，始终可见。

---

## [0.2.7] - 2026-10-09

### 新增
- **CLI `--shortcut <版本>`**：无头创建桌面快捷方式（Windows `.lnk` / Unix `.desktop`）。
- **CLI `--copy-shared <版本>`**：把共享 home 数据复制到某隔离版本的独立 home（需先开隔离）。
- **CLI `--clear-isolated <版本>`**：清空某隔离版本的独立 home（保留版本本身）。
- **CLI `--export [<文件>]`**：导出配置 + 版本清单（JSON），便于换机迁移。
- **界面「导出配置」按钮**：路径设置区一键导出（含保存对话框）。

以上 CLI 命令均支持 `--dry-run` / `--json`。

### 改进
- **环境检查项名支持多语言**：检查项名（Node.js / pnpm / 根目录可写 / 磁盘空间 / npm 源连通）
  随界面语言切换（此前硬编码中文）。
- **`--doctor` 增加「孤立 webview 缓存」检查项**：报告版本已卸载但 webview 目录残留的情况
  （只报告不误删，可用 `--clean` 清理）。

---

## [0.2.6] - 2026-10-09

### 新增
- **更新提醒（C1）**：启动后后台查询 GitHub Releases 最新版本，有新版则在界面顶部显示横幅，
  提供「下载更新」（exe 直链）与「查看更新说明」；可关闭。网络失败静默，不打扰用户。
  （**不含**自动下载安装——那是 C2，另行评估。）
- **CLI `--doctor`**：一键诊断，聚合检查数据根目录 / 各子目录 / 失败版本标记 /
  已安装版本 / 环境（Node / pnpm / 磁盘 / npm 源）；有致命项失败时**退出码 1**；支持 `--json`。
- **CLI `--isolate <版本> [--off]`**：无头开关数据隔离（默认开启，`--off` 关闭），
  与 GUI 行为一致（改配置 + 重生成转发脚本）；支持 `--dry-run` / `--json`。
- **CLI `--clean`**：清理孤立 webview 缓存 **+ 清空卸载回收站**（`--maintenance --cleanup` 的超集）；
  支持 `--dry-run` / `--json`。
- **CLI `--info <版本>`**：查看单个版本详情（路径 / 默认 / 隔离 / 配置目录 / 占用），
  支持 `--json` 输出完整对象（含共享/独占大小）。
- **CLI `--versions`**：列出远端可用版本（从 npm 查询，最新在前），支持 `--json`。
- **CLI `--env`**：无头环境检查（Node / pnpm / 根目录可写 / 磁盘 / npm 源连通），
  有致命项失败时**退出码 1**（便于脚本 `if dsh-multiver --env; then ...`）；支持 `--json`。
- **复用官方 dsh 配置目录（~/.dsh）**：路径设置新增开关，开启后非隔离版本的 `DSH_HOME` 指向
  官方默认目录（Windows `%USERPROFILE%\.dsh`，macOS/Linux `~/.dsh`），
  用户从"裸装 dsh"迁移过来时，已有的会话 / 凭据 / 插件**立即可用**。
  - 默认**关闭**（保持既有行为，不影响现有用户）
  - 隔离版本的优先级更高（开了隔离的版本永远用自己的目录）
  - 切换后**自动重生成终端转发脚本**，保证 GUI 与终端 `dsh` 的 DSH_HOME 一致
  - 首次开启有确认提示（明确"不会删数据、随时可关"）
  - 界面显示官方配置目录的实际路径
  - 详见 [docs/DSH_HOME编排方案.md](docs/DSH_HOME编排方案.md)

---

## [0.2.5] - 2026-10-09

### 新增
- **CLI `--json`**：结构化输出（机读）。`--list --json` 返回版本对象数组；
  `--which --json` 返回 `{version, path}`；破坏性命令（含 `--dry-run`）也可输出 JSON。
  **只影响输出格式**——不带 `--json` 时输出与之前完全一致（现有脚本不受影响）。
- **CLI `--which`**：输出默认版本 dsh 入口的绝对路径（脚本里可直接 `DSH="$(dsh-multiver --which)"`）。
  无默认版本 / 入口不存在 → 退出码 1。
- **繁体中文（zh-TW）与日语（ja-JP）**：界面语言从 2 种增至 **4 种**
  （简体中文 / 繁體中文 / English / 日本語）。系统语言识别自动匹配（zh-Hant / zh-TW / zh-HK → 繁體）。

### 说明
- 本次为**纯功能增量**：无破坏性变更，CLI 默认输出、界面默认语言/主题行为均不变。

---

## [0.2.4] - 2026-10-08

### 修复
- **Linux：内嵌窗口「重新连接中」**（GAP-008）—— 根因是 `jobobj.rs` 在 Unix 侧设置了
  `PR_SET_PDEATHSIG=SIGKILL`。该信号绑定的是**发起 spawn 的线程**（`spawn_blocking` 的池线程），
  命令返回后线程被回收，内核随即杀掉 dsh 子进程，导致内嵌窗口所有 API 请求失败、常驻「重新连接中」。
  此问题**影响所有 Linux 环境**（非 WSLg 专属），Windows 因用 Job Object 而不受影响。
  **修复**：Unix 侧移除 `PR_SET_PDEATHSIG`，只保留 `process_group(0)`（供 `killpg` 清理）。

### 新增
- **CLI `--dry-run`**：破坏性命令（`--install` / `--uninstall` / `--set-default` / `--maintenance`）
  支持 `--dry-run`，只打印将要做的事、不实际执行。适合确认参数是否正确。
- **多语言（中文 / English）**：界面文案全部支持中英切换，可「跟随系统」或手动指定；
  偏好存本地，首屏渲染前应用（无闪烁）。CLI 输出保持中文（面向脚本，稳定优先）。
- **暗色主题**：支持「跟随系统 / 亮色 / 暗色」三态，可手动切换；偏好存本地。
  全部界面颜色收敛为语义化 CSS 设计令牌（`--text` / `--panel-bg` …），暗色仅覆盖令牌、布局不变。
- **「外观与语言」设置区**：位于「路径设置」底部，含主题与语言两个下拉。
- **残留进程兜底清理**（`procreg.rs`）：Unix 侧把 dsh 的进程组 ID 登记到 `<数据根>/logs/dsh-procs.json`，
  管理器启动时清理仍存活的残留进程（覆盖"管理器被 `kill -9` / 崩溃、来不及清理"的场景）。
  正常关窗 / 退出仍走 `killpg`，登记随之注销。
  - **双层身份校验**（防误杀）：条目记录写入者与 dsh 双方的**进程身份令牌**
    （Linux 取 `/proc/<pid>/stat` 的 `starttime`）。清理时：
    ① 写入者仍存活 → **跳过**（避免同一数据根下**多开管理器互杀对方的 dsh**）；
    ② 仅当目标身份令牌与登记时一致才发 `SIGKILL`（杜绝 **PGID 被内核回收复用**后误杀无关进程组）。
  - **读-改-写加文件锁**（`flock`）：修复并发 `register` 丢更新、以及启动清理清掉窗口期新登记的竞态；
    清理只移除已处理条目，不再整体清空（保留活跃实例的登记）。
  - 身份令牌读不到时（进程已死 / 平台不支持）**一律保守跳过**——宁漏杀，不误杀。macOS 暂返回 `None`（未适配）。

### 修复（补充）
- **终端转发脚本加 Node 守卫**（GAP-012 延伸）：`~/.local/bin/dsh`（Unix）与 `dsh.cmd`（Windows）
  此前不做 Node 能力检查 —— 当用户的默认 `node` 仍是 22.16 时，`dsh --version` 会**静默无输出、
  exit 0**（纯 CLI 用户遇到"命令没反应"）。现在转发脚本在调用 dsh 前先做 `import.meta.main`
  特性探测，不满足则打 stderr 并退出 1（与 GUI 环境检查一致）。
- **环境检查的 Node 下限判错**（GAP-012）：`envcheck.rs` 此前只比较**主版本号**（`>= 22`），
  于是 **Node 22.16 会"检查通过"但实际跑不起 dsh** —— dsh 入口依赖 `import.meta.main`
  （22.18/22.19 一线回溯支持），缺失时 dsh **零输出、静默退出**，表现为内嵌窗口连不上，
  极易被误判为 PDEATHSIG 或前端兼容问题（与 GAP-008 症状相同）。
  现改为**双层判断**：① 版本号需 `^22.19.0` 或 `>=24`（与文档一致）；② 特性探测
  `node -e "typeof import.meta.main"` 兜底，防发行版 backport / 版本号骗人。
- **改数据根目录后，旧 root 的残留不被打扫**（GAP-011）：`set_root` 切换前对旧 root 调一次
  `procreg::cleanup_stale`，避免旧 root 下的 dsh 永久残留。

### 测试
- 新增回归测试 `jobobj::tests::gap008_pdeathsig_kills_child_on_thread_exit`：
  对照实验证明「无 PDEATHSIG → 子进程存活；有 PDEATHSIG → 子进程被杀」。
  测试数：Windows 68 / **Linux 78**，全通过。

### 说明
- **集成验证局限**：开发机的 WSL 上 `dsh web` 自身无法启动（node v20 低于 dsh 要求的 22），
  故"真实 dsh web 进程存活"的端到端验证未完成；根因与修复由单元级对照实验证明。

---

## [0.2.3] - 2026-10-05

### 修复
- **任务栏图标各归各位**：管理器窗口显示管理器图标、运行中的 dsh 窗口显示 dsh 图标
  （此前两者颠倒——管理器显示 dsh 图标、dsh 窗口显示管理器图标）。根因是多层叠加：
  Tauri 在 setup 回调之前就创建了主窗口，而进程级 AppUserModelID 必须在任何窗口创建之前设置，
  导致管理器进程的 AUMID 从未生效，任务栏退化为"找指向该 exe 的快捷方式取图标"而被污染；
  dsh 内嵌窗口与管理器同进程、共用进程级 AUMID；快捷方式设 AUMID 时误用了不支持该属性的 WScript.Shell。
  修复：进程 AUMID 提前到 run() 开头；dsh 窗口用窗口级 AUMID（COM 的 IPropertyStore）+ 显示前设置；
  快捷方式改用 COM 设置 AUMID。
- **更新说明加载慢**：此前每次展开某版本的更新说明都要重新请求 GitHub（缓存写在组件实例里，
  每次挂载都是空的）。改为**模块级缓存**，同一版本在任何入口看过一次后即秒出。
- **根目录含首尾空格**：`resolve_root` 此前"判空用 trim、取值用原值"，若用户把数据根目录
  设成末尾带空格的路径，会拼出带空格的子目录。现统一去首尾空格。

### 变更
- **桌面快捷方式使用 dsh 官方图标**：创建 `DSH <版本>.lnk` 时用 dsh 官方图标（不再是管理器图标），
  语义更准确。
- **路径设置支持「展开全部目录」**：默认只显示 5 个核心目录
  （versions/home/store/cache/state），点「展开全部目录」可显示 logs/webview/trash/assets。

### 重构（无功能变化）
- **前端 `App.vue` 拆分完成**：单文件从 1336 行降到 173 行，按职责拆为 6 个组件
  （NotesPanel / EnvCheck / IsolatedMenu / InstalledList / RemoteList / PathSettings）、
  3 个 composable（useToast / useSharedHomeWarn / notesCache）与全局样式 common.css。
  纯结构调整，行为不变。

---

## [0.2.2] - 2026-10-04

### 新增
- **查看版本更新说明**：点「可用版本」的版本号 chip，或点「已安装版本」的版本号，
  可就地展开该版本的更新说明（数据来自 dsh 上游 GitHub Releases，Markdown 已由 GitHub 渲染为 HTML，
  经白名单消毒后展示）。含「在浏览器打开原文」按钮；拉取失败时静默降级为只显示链接。
  标题栏整行可点收起。
- **dsh 启动后崩溃主动提示**：若 dsh 输出了地址、窗口也开了，但进程随即崩溃，
  建窗后约 3 秒做一次「窗口仍开 + 进程已退出」的双条件检查，命中则弹提示并附上 dsh 的真实错误
  （读最新会话日志的 `[err]` 行）。用户关窗 / 重启不会命中，误报极低。

### 变更
- **卸载改为「秒删」**：点「卸载」时先把版本目录 rename 到 `trash/`（瞬时完成，版本立即从列表消失），
  实际删除交给后台线程 / 下次启动的维护任务。Windows 上删 26000 个小文件受 NTFS + Defender 拖累、
  原本要等几十秒，现在用户感知为**瞬时**。
  - 新增 `trash/` 目录；启动维护与手动「清理孤立缓存」会一并清空回收站
  - 若 rename 失败（跨卷等）自动回退为直接删除，保证功能不退化
  - CLI 的 `--uninstall` 同步采用此策略，保持"秒退"

### 修复
- **dsh 启动失败时展示真实错误**：不再笼统提示"输出格式可能已变化"，而是附上 dsh 的实际 stderr
  （例：老版本 dsh 的 `cannot resolve profile bundle ...`）
- **进程表锁中毒不再 panic**：`ProcMap` 锁中毒时忽略中毒继续使用（`lock_procs` helper），
  避免单个任务 panic 拖垮整个应用
- **Linux 桌面快捷方式**：在无桌面环境（WSL/服务器/容器）时给出清晰错误提示，
  不再报晦涩的 `No such file or directory (os error 2)`

### 文档
- **README / 使用手册同步 Linux 支持**：下载表加 Linux 产物、环境要求分平台、
  使用手册「Linux 支持」章节改为"已发布"、构建命令加 Linux 分支
- **新增《征求协助》**：列出非 Windows 平台的待验证项（真机 Linux、macOS），欢迎社区协助
- **开发缺口**：新增 GAP-008（WSLg 下内嵌窗口"重新连接中"的调查结论）、
  GAP-009（前端 App.vue 拆分，暂缓）

---

## [0.2.1] - 2026-10-03

### 变更
- **命令层拆分（GAP-003）**：`lib.rs` 从 971 行降至 190 行——窗口/平台辅助抽出为 `window.rs`，
  全部 `#[tauri::command]` 及专属 helper 抽出为 `commands.rs`（纯移动，无逻辑改动；两平台编译/测试通过）
- **超时错误文案**：启动超时 / URL 解析失败时直接给出实际日志目录完整路径，便于用户直达（GAP-004）

### 测试
- 单元测试 41 → 57：新增命令层纯逻辑覆盖（`build_topbar_script` / `resolve_home` /
  `Config::apply_install_result` / `Config::remove_version_refs` / `Dirs::resolve`）

---

## [0.2.0] - 2026-10-02

### 新增
- **可观测性（日志系统）**：新增会话日志（每次运行版本的完整 stdout/stderr，保留 20 份）、
  `app.log`（主进程错误）、`frontend.log`（前端 JS 错误）；详见 [问题日志查看指南](docs/问题日志查看指南.md)
- **界面显示自身版本号**：顶栏标题旁显示 `dsh-multiver vX.Y.Z`
- **CI**：新增 push/PR 触发的测试工作流（Windows + Ubuntu 矩阵，编译 + 单元测试）

### 变更
- **跨平台（Linux）代码就绪**：进程管理（进程组 + PDEATHSIG / killpg）、转发脚本、CLI TTY、
  脚本目录、桌面集成（.desktop）等均补齐 `cfg(unix)` 分支；已在 Debian 13 上完成
  编译 / 单元测试 / GUI 启动 / CLI / 转发脚本 / 进程清理的验证
- **启动超时 30s → 90s**：缓解 dsh 首次（冷）启动较慢导致的"首次失败、再点成功"

### 修复
- **应用图标**：移除 dsh 官方 logo 图形，改用独立的"层叠版本"图标（规避商标风险）

---

## [0.1.8] - 2026-09-30

### 新增
- **全套无头 CLI**：带任一 CLI 参数即**不启动图形界面**，执行后按退出码退出（成功 `0` / 失败 `1`），便于脚本化与批量运维
  - `--list` —— 列出已安装版本（一行一个；默认版本以 `* ` 前缀标记），输出可直接管道
  - `--install <版本> [--registry <url>]` —— 无头安装，支持指定 npm 源
  - `--uninstall <版本>` —— 无头卸载（同步清理配置与 WebView2 缓存目录）
  - `--set-default <版本>` —— 设为默认版本，并重生成终端 `dsh` 命令
  - `--maintenance [--cleanup | --prune]` —— 无头维护（不带子开关则两者都做）
  - `--help` / `--version` —— 帮助 / 版本号
- **控制台自动接入**：release 版是无控制台的 GUI 程序，CLI 模式下自动 `AttachConsole` 父控制台并切 UTF-8 输出代码页，因此在 cmd / PowerShell 里可直接看到输出；输出被重定向到文件/管道时保持原样

### 变更
- **安装进度走 stderr**：CLI 安装时进度单行刷新（`\r`），stdout 保持纯净可管道；输出非终端（重定向/管道）时自动静默
- **未知参数不再静默启动 GUI**：传了无法识别的 `--xxx` 会报错退出（退出码 1），不再误弹图形界面

### 修复
- 清理 `get_manager_dir` 死代码（自 v0.1.0 起注册但前端从未调用的 Tauri 命令）

---

## [0.1.7] - 2026-09-30

### 新增
- **可用版本搜索与排序**：支持按版本号搜索、切换「最新在前 / 最早在前」；不再限制只显示前 40 个版本，可滚动查看全部
- **扫描占用显示复用/独占**：扫描版本占用时，额外显示「复用多少 / 独占多少」（基于 pnpm store 硬链接统计），让用户了解省了多少磁盘
- **维护面板**：新增维护区，含自动维护开关、规则说明、上次执行状态，以及手动「清理孤立缓存」「回收依赖仓库」「打开日志目录」按钮
- **日志目录入口**：路径设置中可直接打开 `logs/` 目录

### 变更
- **内嵌窗口顶部小栏改版**：从常驻顶部横条改为**左上角极简小点**（默认 8px；鼠标悬停展开为药丸显示版本号；点击弹出面板，含版本号、完整 URL、重启按钮），不再遮挡 dsh 页面
- **点可用版本改为填入输入框**：点击版本号不再直接安装，而是填入输入框，需再点「安装」才执行（避免误点）
- **维护状态迁入 config.json**：维护相关配置与状态（自动维护开关、上次清理/回收时间）集中到 `config.json` 的 `maintenance` 字段
- **可用版本列表高度压小**：从 220px 压到 120px，节省界面空间
- 维护按钮改为蓝色描边，明确「可点击」

### 修复
- **主界面白屏**：App.vue 缺失 `computed` import，导致界面无法渲染
- **维护按钮被误禁用**：`disabled` 绑定传了字符串 ref（空字符串被当作禁用），修复为布尔判断

---

## [0.1.6] - 2026-09-30

### 新增
- **批量操作**：已安装版本列表支持多选，可全选本组、清除选择、批量卸载（一次确认，逐个执行，实时进度，失败项汇总）
- **后台磁盘维护**：启动时自动清理孤立的 webview 缓存目录；距上次 ≥ 7 天时低优先级运行 `pnpm store prune` 回收未引用的包

### 变更
- 卸载版本时**一并清理 webview 数据目录**（`<根>/webview/<版本>`），不再残留缓存
- 更新 README，补齐 v0.1.5~v0.1.6 新功能说明并重截主界面

### 修复
- 批量卸载的禁用范围：仅「卸载」按钮 + 被选中条目阻塞，非选中条目的其它按钮不受影响

---

## [0.1.5] - 2026-09-29

### 新增
- **安装进度条**：8 阶段加权进度 + pnpm 依赖总数细分（实时显示安装到哪一步、进度百分比），防阶段回退
- **安装失败换源重试**：失败按类别提示（网络 / 私有包下架 / 版本不完整），网络问题可一键切换 npm 源（官方 / 阿里云 / 腾讯云 / 华为云 + 自定义）重试
- **失败版本标记**：依赖已下架的版本在列表中标灰划线，避免反复踩坑；支持手动清除标记
- **环境检查引导**：缺少 Node / pnpm 时，直接给出下载链接与安装步骤（跨平台 `open_url`）
- **卸载状态反馈**：卸载按钮禁用 + 显示「卸载中...」

### 变更
- 卡片两端对齐 / 换行左对齐 + 按钮防竖排 + 窗口尺寸调整

---

## [0.1.4] - 2026-09-29

### 新增
- **根除孤儿进程**：用 Windows Job Object 绑定 dsh 子进程，管理器退出（正常 / 强杀 / 崩溃）时由系统自动清理所有 dsh 进程，不再残留占资源、锁文件
- **共享 home 提示**：非隔离版本在列表中显示「共享 home」标记；运行前弹提示（多版本混用可能导致插件/依赖错配，建议对测试版开隔离），可在「路径设置」中关闭
- **版本信息展示**：列表显示安装日期；支持「扫描占用」查看版本目录大小

### 变更
- **确认对话框改用系统原生对话框**：Tauri WebView2 中 `window.confirm` 静默失效，导致「卸载」「清理隔离数据」「复制共享数据」「运行非隔离版本」等所有需确认的操作点了没反应
- 消除误导性终端黑框：创建快捷方式、环境检查、刷新版本列表、安装版本时不再弹终端窗口（仅「浏览器打开」保留，以便查看 URL/token）
- 添加 `.gitattributes` 统一换行符；清理死代码

---

## [0.1.3] - 2026-09-28

### 修复
- **任务栏图标丢失与桌面快捷方式创建失败**：通过显式设置 AppUserModelID（AUMID）+ 注册表 IconUri 修复
- **`pnpm add` 报 Unknown option: state-dir**：改用 `--config.xxx=value` 形式传目录选项

---

## [0.1.2] - 2026-09-28

### 新增
- **内嵌窗口运行**：在应用内直接打开 dsh Web 界面，每个版本使用独立的 WebView2 数据目录
- **桌面快捷方式 + 多开**：为版本创建桌面快捷方式，双击直达；支持同时运行多个版本
- **终端 dsh 统一 DSH_HOME** + 浏览器打开固定端口 3080（占用时弹框选择）

### 修复
- **内嵌窗口 431 / Failed to load plugins**：每个版本隔离 WebView2 数据目录 + 设浏览器 UA
- **二次运行报「label already exists」**：窗口 label 唯一化（带代次）
- **隔离版本启动崩溃**：复制共享数据时保留 junction（`mklink` 分开传参）
- **`pnpm view` 不认 `--store-dir` 等**：改用 `--config.*` 形式

### 变更
- 所有 pnpm 目录（store/cache/state）限定在数据根目录内，避免污染全局

---

## [0.1.0] - 2026-09-27

### 初始版本
- 多版本共存：每个版本独立安装在 `versions/<版本号>/`
- 省磁盘：pnpm store 硬链接去重
- 自定义存储位置（默认 exe 同目录）
- 版本操作：安装 / 卸载 / 设默认 / 一键运行
- 终端短命令 `dsh`（设为默认后生成转发脚本）
- 环境检查（Node / pnpm / 目录可写 / 磁盘 / npm 源连通）
- 数据隔离（可选）：每个版本可用独立 DSH_HOME
