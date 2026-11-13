# 更新日志

本文件记录 DSH 版本管理器（dsh-multiver）各版本的变更。
最新版本的详细说明见 [Releases](https://github.com/K1-lihongrong/dsh-multiver/releases)。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

---

## [0.4.0] - 2026-11-13

### 同版本多实例

- 新增全局实例表：instances / version_instances / selected_target / instance_seq；一份 homes/<id> 可挂多个版本。
- 配置升级到 schema 2；旧 isolated_versions 自动迁移为「默认隔离」实例，迁移中断可重试。
- CLI 新增 instance create/list/rename/remove；isolate 增加 --list / --attach / --detach；run / copy-shared / clear-isolated 支持 --instance。
- GUI 主按钮改为分体按钮 + 实例浮层；新增全局实例面板（孤儿置顶、挂载/解除、改名、删除）。
- 同版本多实例可同时开窗，关 A 不影响 B；实例运行态跨版本显示。
- 备份导入导出覆盖实例表；旧备份自动合成默认实例。

### 行为变更（破坏性）

- isolate --off 语义改为「解除该版本全部实例挂载」，实例本体保留。
- 隔离 home 从 versions/<版本>/home 改为 homes/<id>。
- WebView2 数据目录改为 webview/<版本>/_shared 与 <版本>/<id>，旧目录自动改名。
- 卸载版本不再删除仅属于该版本的实例数据。
- instance list 默认不扫占用，新增 --sizes 显式开启。

### 修复

- 迁移目标已存在时复用，源目录保留并写日志。
- selected_target 缺失/null 回落共享 home。
- isolate 不重置显式选择的主按钮目标。

### 验证

- Rust 单测 271 passed / 1 ignored；fmt、clippy、pnpm build、check-commands、check-i18n 通过。
- E2E 71 passed（新增多实例浮层、运行参数、跨版本运行态、实例面板与窄视口菜单用例）。

### Linux

- 明确不打包、不发布 Linux 产物；仅保留 CI 编译/测试，GAP-020 暂缓。

## [0.4.0-alpha.3] - 2026-11-11

### 一键更新修复

- 修复一键更新后管理器退出但不重启、手动打开仍是旧版的问题。
- 根因是 detached 隐藏窗口里的替换批处理使用 `tasklist | findstr` 等待进程退出，`findstr` 会卡在管道 EOF，永远走不到复制和重启。
- 改为进程内替换自身 exe：复制到目标同目录临时名，旧 exe 改名 `.old`，再改名到目标；任一步失败回滚，旧 exe 保持可启动。
- 替换成功后再安排延迟重启；重启脚本不再使用管道和进程探测，退出约 3 秒后启动新 exe，避开单实例锁。
- 暂存更新安装后立即安排重启，不再需要用户手动打开第二次。
- 新增 `DSH_MULTIVER_UPDATE_API_BASE`，用于本地 mock server 更新验收。
- 新增运行中 exe 替换、回滚、重启脚本无管道等回归测试。

**用户可见变化**：更新失败会显示真实错误且不退出；更新成功后自动重启为新版本，不再出现退出后仍旧版。

### A2 技术债收口

- dsh_adapter 拆分为 detect / v0_1 / v0_3 / mod，统一 CLI 入口、profile 目录和版本能力门控。
- 未知版本走安全适配器，不误开 allow-version 等新版本专属能力。
- 新增 JobEventSink、CancellationToken、started_at 和 64 条历史上限；淘汰 Job 时同步清理 token。
- launch、scan、install、插件操作、批量更新和共享 home 体检迁移到统一 job:* 事件。
- 旧进度/取消事件继续保留，扫描取消同时接入 Job token 与旧全局标记。
- 修复插件操作 spawn_blocking 返回类型错误，并收口 clippy 告警。

**用户可见变化**：后台任务进度和取消行为保持兼容；插件命令按 DSH 版本能力门控，未知版本走保守默认。

### 自动化验收

- 新增 `scripts/acceptance-uia.ps1`：真实原生窗口 UIA 验收启动、版本、插件面板、扫描取消、DSH 版本启动/关闭、日志和残留进程。
- 新增 `scripts/acceptance-update.ps1`：本地 mock API 驱动的真实更新链路验收，覆盖 SHA256、替换、`.old` 备份、退出和自动重启。
- 新增 `scripts/compat-matrix.ps1`：11 版本隔离兼容矩阵，默认 dry-run，`-Execute` 才执行。
- `package.json` 增加 `test:acceptance:ui`、`test:acceptance:update`、`test:compat` 入口。

### 兼容矩阵与验证

- 2026-11-11 兼容矩阵回归：11 个 DSH 版本全部通过。
- 每个版本验证 plugin add/remove/add、dsh web 启动、state 200/ok、无 cookie 401、带 cookie GET 405。
- 真实数据根 config.json 的 SHA256 前后一致；测试 home 全部位于 %TEMP%。
- Rust 单测 256 passed / 1 ignored。
- E2E 63 passed。
- cargo fmt --check、cargo check --all-targets、cargo clippy --all-targets -D warnings、pnpm build 通过。

---

## [0.4.0-alpha.2] - 2026-11-09

### 第一批

- 新增共享 home 体检：只读扫描 profiles/node_modules 的旧版本 junction、悬空链接和重复来源，不自动删除。
- 新增插件安装失败分类器：覆盖版本兼容、发布保护期、文件锁、网络、环境、peer 警告、构建脚本、未知 8 类；完整保留 pnpm/dsh 原始输出。
- 新增 JobRegistry 基础、`list_jobs` 命令；批量更新开始发 `job:*` 事件。
- 新增 dsh_adapter 基础识别与 allow-version 能力门控。
- GUI/CLI set_root 共用路径校验。
- 新增 install_service 完成账本，GUI/CLI 共用 broken_versions 维护。
- 单插件安装/卸载接入 `job:*` 事件。
- 更新完成后不再自动触发一次完整检查更新，避免「添加插件 / 检查更新 / 体检」按钮长时间置灰；需要刷新时手动点「检查更新」。

---

## [0.4.0-alpha.1] - 2026-11-09

### 类型与状态收口

- 引入 `VersionId` / `InstanceId` / `HomeRef` / `ProcKey` / `LaunchTicket`，接入配置、路径、进程表与窗口 label。
- `config.json` 新增 `schema_version`、实例表、挂载表、主按钮目标与实例序号；旧 `isolated_versions` 自动迁移到 `homes/iN`，保留兼容镜像。
- `AppCtx` 统一持有进程表、启动意图、目录快照、扫描取消标志与窗口代次；状态敏感命令已迁移。
- 新增 `Clock` / `FileSystem` / `PnpmRunner` / `DshLauncher` 端口与生产 adapter；`DshLauncher::spawn_command` 已接入内嵌 dsh 启动路径。

### 插件批量更新与失败回滚

- 新增 `ds_plugin_update_batch`：当前版本 + 当前 profile 的可更新插件可一键逐项更新。
- 单插件更新改为 `remove → add → verify` 事务；add 或校验失败时按原依赖 spec 回滚。
- 回滚失败返回原错误、回滚错误与人工恢复线索，不再让插件静默停留在已卸载状态。
- 前端新增「全部更新」按钮、批量进度、成功/失败汇总；官方随附 bundle 不参与批量更新。
- 检查更新改为 `pnpm outdated` + 非官方插件逐项 `pnpm view <pkg> version` 兜底，修复 hoisted profile 漏报 `dsh-ops 0.5.1 -> 0.5.2`；用 semver 比较过滤官方随附包的 prerelease 误报。
- 检查更新和插件操作增加前后端超时兜底：后端限制单次检查时长，前端超时后恢复「添加插件 / 检查更新」按钮，避免永久 busy。
- 检查更新时显示实时耗时与不确定进度条，避免长时间无反馈。
- 安装失败输出中出现不兼容插件时，界面解析出包版本 + DSH 版本，一键调用官方 `dsh plugin allow-version ... --accept-risk` 后重试；profile 信息区可查看/撤销已有精确版本豁免。
- 单插件「更新」也复用事务命令，不再走前端裸 remove+add。

### 验证

- Rust 单测 **227 passed / 1 ignored**
- E2E **61 passed**
- i18n 5 语 × 386 key
- `cargo fmt --check`、`cargo clippy -D warnings`、`pnpm build`、`pnpm tauri build --no-bundle` 通过

## [0.3.7] - 2026-11-09

### 安全与契约加固

修复 v0.3.6 引入的内嵌 dsh 窗口事件桥越权路径，并补齐 v0.3.7 的验证保险。

- **A1 窗口级事件监听**：删除全局 `app.listen_any`，四个 topbar 监听器改到 `launch_window` 内用 `window.listen` 绑定；版本号由闭包捕获，payload 的 `version` 不再被采信。
- **A1 来源认证加强**：topbar 改用 `emit_to` 当前窗口 label；每窗口注入随机 128-bit token，四个监听器校验 token，跨窗口伪造事件被拒绝。
- **A2 capability 拆分**：删除合并版 `default.json`，新增 `main.json` 与 `dsh.json`；内嵌窗口只保留 `core:event:allow-emit-to` + `core:window:allow-start-dragging`，移除 `core:default` / `opener` / `dialog`。
- **A3 LaunchIntent**：三个启动状态 static 合并为 `LaunchIntent`；写入/读取时机与语义不变。
- **A4 i18n 校验**：新增 `scripts/check-i18n.mjs` 并接入 CI；5 语各 361 key。
- **A5 契约测试**：新增 `src-tauri/tests/contracts.rs` 与独立 CI job，覆盖 fake pnpm 输出、错误分类、版本 JSON 解析、真实 GBK 解码与真实 pnpm 对拍。
- **A6 staged 失败路径**：新增 `src-tauri/tests/staged_failures.rs`，覆盖目标缺失、rename 失败、放入失败回滚与成功路径。

**用户可见变化**：无。两版本并行时，topbar 操作只作用于当前窗口。

### 验证

- Rust 单测 **202 passed / 1 ignored**
- staged 失败路径 **4 passed**
- 契约测试 **4 passed / 1 ignored**；真实 pnpm 对拍 **1 passed**
- clippy 0 warning；fmt 通过；`pnpm build` 通过；i18n 5 语 × 361 key 通过
- `pnpm tauri build --no-bundle` 本地打包通过

---

## [0.3.6] - 2026-11-06

### 内嵌 dsh 窗口标题栏重做

内嵌 dsh 窗口原本由系统绘制标题栏，与 dsh 页面主题不协调；原 `topbar.js` 仅在页面左上角显示一个小圆点，信息量弱。本次改为自绘整条标题栏。

- 窗口设 `decorations: false`，由 `topbar.js` 自绘：版本号 + URL（可点击选中复制）+ 重启 + 最小化/最大化/关闭。
- 配色跟随 dsh 页面深浅色（观察 `body[data-ds-dark-theme]`，250ms 防抖），同步设置 DWM 深色模式（属性 20 失败回退 19）。
- **通道改造**：内嵌窗口是 remote 上下文，Tauri 2 默认拒绝应用自有命令（`Command not allowed by ACL`）。改为统一 emit 事件（`topbar-log` / `topbar-restart` / `topbar-ctrl` / `topbar-theme`），Rust 侧 `listen_any` 接收。顺带修正了旧「重启」按钮一直静默失败的隐患（之前 catch 兜底 `location.reload()`，从未真正重启过进程）。
- 不修改 `build.rs` 的 `AppManifest::commands()`（白名单模式会连带失效主界面命令）。

**用户可见变化**：内嵌 dsh 窗口只有一层标题栏（不再与系统标题栏叠两层）；标题栏显示完整版本号与 URL，并随 dsh 主题切换深浅色；「重启」按钮真正重启 dsh 进程。

### 安装进度平滑化

安装 dsh 版本时进度条长时间停在同一百分比后跳格，观感卡顿。原因是后端事件节流阈值过粗（fraction 变化 ≥ 0.01 才推送），而 pnpm 在写入文件阶段输出不均匀，大量硬链接写入时可能几百毫秒无输出，一旦输出就跳多个包；前端进度条也无插值。

- 后端 `install.rs` 节流阈值 `0.01` → `0.003`，事件更密。
- 前端 `RemoteList.vue` 在两次事件之间按 200ms 步长线性插值爬升，只追不改——到达目标即停，下一事件把目标推高再继续，不越过真实进度。
- 进度条过渡改为 `.2s ease-out`，与爬升步长对齐。
- 阶段权重表（`STAGE_START` / `STAGE_WEIGHT`）未动，整体节奏不变。

**用户可见变化**：安装进度条推进更连续，不再长时间停在同一百分比后跳格。

### 推荐页分类标签多语言

切到韩语（或英、日、繁中）后，插件推荐页的分类标签仍显示中文。

- `docs/plugins.json` 新增 `categoryI18n`（8 个分类 × 4 语言），9 个插件补 `ko-KR` 描述，`updatedAt` 更新为 2026-11-04。
- `catalog.rs` 的 `RawCatalog` 与 `Catalog` 新增 `category_i18n`，透传给前端；旧 catalog 无该字段时 `serde(default)` 为 `None`，行为与现在一致。
- `DsPluginCatalog.vue` 分类 label 按当前语言取 `categoryI18n`，缺项回退中文；**key 保持中文原值不变**（`catalogItems` 用它做相等匹配，改 key 会破坏筛选）。
- `currentLang` 补 `ko-KR` 分支并上移到 `catalogCategories` 之前，消除 TDZ 隐患。

**用户可见变化**：韩语 / 英语 / 日语 / 繁中界面下，插件推荐页的分类标签跟随界面语言显示。

### dsh 版本兼容矩阵补测（GAP-018 闭环）

补测 10 个 dsh 版本，`verifiedDshVersions` 从 5 个扩到 9 个。

- **向上补测 4 个全部完全支持**：`0.2.1-rc.4`、`0.2.1-rc.5`、`0.2.2-alpha.2`、`0.2.2-alpha.3`。
- **向下探边界 6 个全部「无法判定」**：`0.1.5-rc.2`、`0.1.2-alpha.5`、`0.1.1-rc.2`、`0.1.0-rc.8`、`0.1.0-rc.2`、`0.0.1-rc.5` 均命中同一个 dsh 自身缺陷（`user patch-layer watching requires the Cordis HMR service`），与插件无关。
- **最低已验证版本维持 `0.1.5-rc.3`**：不是插件不支持更早版本，而是 `0.1.5-rc.2` 及以下都因该缺陷无法启动。该缺陷横跨 `0.0.1-rc.5` ~ `0.1.5-rc.2`，在 `0.1.5-rc.3` 修复，向下已到头。

### 安装进度条中途不推进（真因修复）

前次"进度平滑化"只压低了节流阈值，实机验收发现进度条仍长时间不动、再跳格。追查后确认是另外两个独立缺陷，与解析逻辑无关（曾误判为 pnpm 输出带路径前缀）：

- **后端未用 append-only reporter**：pnpm 默认 reporter 只发「开始（added 0）」与「完成（added N, done）」两条 Progress 行，阶段内 fraction 无从推进，进度从 4% 直接跳到 90%。改用 `--reporter=append-only`，实测输出 13 行（含下载/写入中间态），阶段内能真正递进。
- **前端爬升定时器反复重建**：`startClimb()` 每次进度事件都先 `stopClimb()` 再 `setInterval`，事件密集时定时器永远等不到第一个 200ms tick，插值实际失效。改为已有定时器时直接返回。

**用户可见变化**：安装进度条连续爬升，过程中可见中间百分比，不再从 4% 跳到 90%。

### 隐藏系统标题栏（自绘顶栏）

管理器主窗口改为无边框（`decorations: false`），顶栏自绘最小化/最大化↔还原/关闭三键，随主题变色；加载页可拖动、右上角可关闭。

- 顶栏 `mousedown` 设 4px 拖动阈值后才调 `startDragging()`，避免吞掉双击最大化。
- Tauri 2 的窗口操作需显式权限，`capabilities/default.json` 补 7 条 `core:window:allow-*`；此前缺失导致三键点击完全无反应（异常又被 `catch` 静默吞掉），现已改为失败写 `logs/frontend.log`。
- 任务栏图标（AUMID）不受影响。

**用户可见变化**：主窗口无系统标题栏，顶部为应用自绘三键；最小化、最大化/还原、关闭均可用。

### 验证

- Rust 单测 **198 passed / 1 ignored**（新增 catalog 3 条、切行流式回归 1 条、topbar 前后端契约断言 1 条）。
- E2E **59 passed**（新增 `d36-catalog-i18n` 4 条）。
- `cargo clippy` 0 warning；`cargo fmt --check` 通过；`pnpm build` 通过。
- 内嵌 dsh 窗口实机验证：浅色/深色渲染、重启（窗口原地刷新）、拖拽、三键、关闭（单窗口退出 + 多窗口主界面存活）全部通过。

---

---

## [0.3.5] - 2026-11-04

### 韩语词条包（ko-KR）

新增韩语界面。`src/composables/i18n/ko-KR.js` 376 行，覆盖全部界面文案。

- `useI18n.js` 注册 `ko-KR` 到 `MESSAGES`，`LANGS` 增加一项（语言下拉多一个选项）。
- `systemLang()` 增加 `ko` 前缀识别：韩语系统首次启动自动选韩语。
- `PathSettings.vue` 语言下拉新增「한국어」。

**用户可见变化**：界面语言多一个「한국어」选项，韩语系统用户首次启动自动使用韩语。

### 手册配图改为动态 WebP

使用手册的插图原本是 v0.2.8 时拍的静态截图，跨了 9 个版本未更新，界面已明显不符（缺插件按钮、缺启动加载页、按钮动效未体现）。改为 Playwright 录屏转码的动态 WebP（当前 16 张），同时解决"静态图看不到交互反馈"的问题。

- **录屏管线**：新增 `playwright.record.config.js`（1280×800、workers=1、video on）与 `e2e/record/_rec.js`（假光标、平滑移动、按元素点击、就绪等待）。Playwright 的 `recordVideo` 不录系统鼠标指针，观众看不到点了哪里，故注入一个跟随 `page.mouse` 的 div。
- **录屏 spec**：`e2e/record/` 下的场景脚本（01 主界面 ~ 12 路径设置，后续补入 13~16 插件与外观），复用 `e2e/helpers.js` 的 `mockTauri`。全部在纯前端（`vite preview`）下跑，不需要 Tauri 后端。
- **补事件通道**：`helpers.js` 原先只 mock 了 `invoke`，没有 mock 事件。`listen()` 走 `invoke('plugin:event|listen')`，故补上登记表并暴露 `window.__mockEmit(event, payload)`，供安装进度图推送 `install-progress` 事件。
- **补默认假数据**：`list_remote`（13 个版本号）、`scan_usage` / `scan_version_size`（占用数字）。
- **转码脚本**：`e2e/record/` 下的转码脚本，`-ss 0.8` 裁掉启动白屏与启动页，宽度 960，输出到 `docs/screenshots/manual/`。后续统一转 WebP（GIF 体积大且色深受限）。
- **顶栏圆点（10）**：`topbar.js` 是注入到内嵌窗口的纯 DOM 脚本，不依赖 dsh 后端。直接注入到空白页并模拟悬停/点击，录出 dot → pill → 面板三态，无需实机录屏。
- **删除 12 张旧图**（11 个 `.jpg` + 1 个 `.png`），手册引用同步改为动态图。
- **新增 `docs/screenshots/manual/README.md`**：说明重新生成的步骤与已知坑。

**验证**：录屏 spec 全通过；动态图转码成功，体积 100~503 KB；抽帧核对 01/02/05/06/07/08/10 内容与预期一致；07/08 首版因等待时机过早未弹出对话框，改为等 `.modal-mask .modal` 可见后重录通过。

**用户可见变化**：使用手册的插图从静态截图换成动态图，可看到悬停、点击、进度推进等交互反馈。

### 文档与规划

- **`docs/v0.3.6规划.md`**：新增 dsh 版本兼容矩阵专项规划（GAP-018），列出待测 13 个版本、判定口径、执行方法与完成标准。本次只落规划，不含实施。
- **`docs/v0.4.0规划.md`**：新增同版本多隔离实例规划。实例从「版本的布尔属性」升级为全局实体，一份 `DSH_HOME` 可挂载到多个已安装版本。本次只落规划，不含实施。
- **`docs/技术债.md`**：D7/D8 实测数据更新至 v0.3.4 基线。
- **`README.md` / `docs/使用手册.md`**：修正过期描述，补插件入口与 Linux 验收范围说明。
- **`docs/开发缺口.md`**：新增 GAP-019（手册动态配图未做界面语种适配）。

### 按钮交互动效全站统一（技术债 D22）

v0.3.3 加强插件面板 hover 反馈时，只改了过渡属性写法，与界面其他按钮仍不一致。排查后发现差异远不止过渡：hover 底色、hover 描边、主按钮变化方向、危险按钮色源、焦点环覆盖范围、按下态有无，共 6 个维度两套并存。

- **新增 4 个动效令牌**：`--motion-fast`（.12s）、`--motion-base`（.15s）、`--ease-standard`（`cubic-bezier(.2, 0, 0, 1)`）、`--press-scale`（.97）。全站过渡一律引用令牌，不再写字面值。
- **hover 统一为浅底 + 主色描边**：底色 `--surface-3`，描边 `--accent-border`。原主界面按钮用深灰描边（`--border-strong`），插件面板用主色描边，同一屏内两种反馈语言。
- **新增按下反馈**：底色再深一档（`--accent-weak-3`）+ `scale(.97)`。此前全站按钮均无 `:active` 样式，点击没有任何落点感。
- **焦点环全站统一**：2px 主色 `outline` + 1px offset，`:focus-visible` 语义（鼠标点击不显示）。此前仅插件面板有，主界面按钮按 Tab 走的是浏览器默认虚线框。
- **主按钮统一为变深 + 光晕**：`.dp-btn-primary` 原先用 `filter: brightness(1.08)`，方向与 `.btn.primary` 的变深相反，且 `filter` 不在 `transition` 内导致瞬变。现统一为背景过渡到 `--accent-hover` 并叠加主色光晕。
- **危险按钮统一为实心红底白字**：`.dp-btn-danger` 原先硬编码 `rgba(229,72,77,.08)`，与 `.btn.danger` 的令牌 `--danger-weak` 不同源。现统一为 hover 时 `--danger` 实心底 + 白字，按下再加 `brightness(.94)`。
- **补齐 6 处原本无过渡的控件**：分段控件 `.dp-seg-btn`、分类滚动箭头 `.dp-cat-nav button`、分类刷新 `.dp-cat-refresh`、日志开关 `.dp-log-toggle`、文字链接按钮 `.dp-link-btn`、提示条关闭 `.toast-close`。此前 hover 是瞬变，与相邻按钮的 120~150ms 过渡并存。
- **`transition: all` 清零**：全库 0 处残留，改为分属性过渡并纳入 `transform`。
- **hover 不改文字颜色**：按钮文字保持 `--text-2`（大按钮）/ `--text`（小按钮）不变。曾考虑 hover 时加深文字，实测在浅灰底上观感"发黑"，放弃。

### 插件面板展开态可见

- **「插件」按钮增加展开态**：面板展开时按钮变为浅主色底 + 主色描边 + 主色文字，左侧三角箭头旋转 90°，收起时恢复。此前按钮点击是切换逻辑（再点一次确实会收起），但展开与收起外观完全一致，用户无从判断当前状态、也不知道再点能收起。面板内部的关闭按钮要看到面板才知道。
- 三角箭头用 `border` 绘制、`content` 留空。**若用文字字符（如 `▸`）会进入按钮的可访问名称**，导致 E2E 的 `getByRole("button", { name: "插件", exact: true })` 匹配失败——首次实现踩到此坑，24 条用例连锁失败。
- 同时补 `aria-expanded`，供读屏软件识别展开状态。

**验证**：`pnpm build` 通过（CSS 47.79 kB）；E2E 38 passed；构建产物 CSS 规则逐条从 `document.styleSheets` 读出核对，13 项全部符合。

### 决策依据

改动前生成三份可视化对比页，逐项确认：现状审计表（7 个维度实测差异）、8 个决策项的交互式选择页、最终方案示例（6 个真实场景）。对比页存于本地 `.cuckoo/`，不进仓库。

---

## [0.3.3] - 2026-11-03

### 移除版本豁免功能

- **「版本豁免」区块已从插件面板移除**。该功能自 v0.3.0 上线起就无法工作——它调用的 `dsh plugin allow-version` 在所有已测 dsh 版本（`0.2.0-rc.2` / `0.2.1-rc.2` / `0.2.1-rc.3`）中都不存在。`dsh plugin` 本身只是把参数转发给 pnpm，没有这个子命令。
  - 豁免机制在 dsh 内部确实存在（`dsh-app-boot` 的 `setProfileVersionExemption`，状态存 `<profile>/compatibility.json`），但没有暴露 CLI 入口。当初据错误提示文本实现，未验证命令能否跑通。
  - 后端 `ds_exemption_action` 命令保留未删。等 dsh 提供 CLI 后按 `docs/开发缺口.md` B1-19 恢复。
  - 同步移除：组件内豁免 UI、4 语言各 10 条 i18n 词条、相关 CSS、2 条 E2E 用例。
- **不再把 `allow-version` 转成「可执行指引」**。安装失败时透传官方原文，只补充诊断日志路径——指向不存在的命令只会误导。

### 插件面板修复与改进

- **修版本豁免命令参数名**（虽该功能已移除，参数映射本身是错的）：Tauri 2 默认把命令参数映射成 camelCase，而 `ds_exemption_action` 的后端参数是 `package_version`，导致 `missing required key packageVersion`。加 `rename_all = "snake_case"`。
- **插件描述支持多语言**：catalog 新增可选字段 `descriptionI18n`，前端按当前语言取用，缺项回退中文原文。`docs/plugins.json` 9 条插件补了繁中 / 英文 / 日文译文。
- **已安装列表改两行排版**：包名独占一行，版本与徽标、操作按钮移到第二行。包名过长时省略号截断、悬停显示全名；卸载按钮推到行尾右对齐。此前长包名被截断，第一版改成换行后观感参差，用户实测后改回单行。
- **按钮 hover 反馈加强**：浅色主题下 `#fff` 到 `#f0f2f5` 色差极小，只换背景几乎看不出。改为同时加深边框；补 `:focus-visible` 焦点环供键盘导航。
- **分类标签栏支持拖动**：按住左键横向拖动，拖动距离超 4px 才算拖拽，避免误吞按钮点击。
- **卸载时显示已运行秒数**：大包卸载时 pnpm 会长时间无输出，加计时器 + 自动展开日志区，缓解「卡死」观感。
- **识别 `ERR_PNPM_CANNOT_REMOVE_MISSING_DEPS`**：包已不在 `dependencies` 但仍在 profile 的 `bundles` 列表时，提示用户去 `package.json` 的 `dsh.profile.bundles` 手动清理。
- **补回窗口 resize 时重算标签栏溢出**：拆分 D21 时遗漏，导致窄窗口不出现 ‹ › 滚动按钮。

### 插件面板拆分（技术债 D21）

- **`DsPluginPanel.vue` 从 1677 行降到 341 行**，按职责拆成四块，互不共享状态的代码不再挤在一个文件里：
  - `DsPluginCatalog.vue`（458 行）：推荐页——分类标签栏、双行卡片、描述浮窗、星标排序、远端检查。
  - `DsPluginInstallDialog.vue`（339 行）：安装对话框五阶段（form → checking → confirm → installing → done/failed）+ 安装源记忆与首次探测。
  - `DsPluginInstalled.vue`（247 行）：已安装列表、筛选、卸载、检查更新、更新。
  - `DsPluginMeta.vue`（52 行）：待决定的构建脚本审批。
- **状态归属按实际共享关系上提**：`installedNames`、`busy`、`lastAction` 留在壳；进度行抽成 `composables/useDsPluginLog.js`，由壳 `provide` 给安装对话框与底部日志区共用。
- **样式独立成文件**：全部 `dp-*` 规则移到 `styles/dsplugins.css`（481 行），由 `main.js` 引入。子组件不需要再各自复制一份 scoped 样式。
- **推荐页改为切到该标签时才激活**，避免拆组件后一挂载就发远端请求。
- 官方失败诊断的本地化提示抽到 `composables/useDsPluginErrors.js`，壳与子组件共用。

### 验收后的修复

- **修内置副本被旧缓存遮挡**：`load_local` 原先「有缓存就用缓存」，不比较新旧。旧版程序写的缓存（没有 `updatedAt`）会永远遮住新版程序内置的副本，导致 `descriptionI18n` 等新字段到不了用户。改为按 `updatedAt` 取较新者，缓存无时间戳视为更旧、回退内置。
- **已安装列表包名与按钮排版**：包名过长时省略号截断（悬停显示全名），卸载按钮推到行尾右对齐。两处均为用户实机验收后的反馈。

### 技术债

- **记录 D22**：插件面板按钮的过渡属性与界面其他按钮不一致（v0.3.3 加强 hover 时把 `transition: all` 拆成了分属性过渡）。用户决定下个版本统一。

### 验证

- `pnpm build` 通过。
- **E2E 38 passed**（`dsplugins.spec.js` 23 条，含新增推荐页的分类筛选、星标排序、描述浮窗、描述随语言切换）。
- **Rust 单测 188 passed / 1 ignored**（新增 catalog 时间戳、i18n 字段、内置副本优先级的用例）。
- `cargo fmt --check` 干净、`cargo clippy -D warnings` 干净。

---

## [0.3.2] - 2026-10-30

### 更新下载的镜像回退

- **新增统一的下载回退机制**（`src-tauri/src/net.rs`）：直连 GitHub 失败时按序切换镜像，任一成功即返回。此前无代理的用户直连 `github.com` 常常超时，`update` 命令形同虚设。
  - 失败判定不只看状态码——公共反代常返回 200 但内容是 HTML 错误页，因此下完后还要校验响应形状（exe 检查 `MZ`/`ELF` 头，JSON 检查首字符）。
  - 每个源有独立超时，直连 20 秒、镜像 20 秒；catalog 的直连超时缩到 5 秒（非关键路径，拉不到就用内置副本）。
  - 镜像列表硬编码，不暴露给用户配置——普通用户不该需要理解什么是反代。
  - 与既有的 Node/pnpm 下载回退（`runtime/bundled.rs`）保持同一思路。

### 推荐插件列表

- **新增推荐插件页**：插件面板工具栏加入「已安装 | 推荐」分段切换，支持一键安装（复用现有安装对话框，不新写安装器）。
  - 单网格布局，与已安装列表共用 `minmax(320px, 1fr)` 自适应列数。
  - 卡片双行：第一行图标 + ⭐ + 包名 + 徽标 + 安装按钮；第二行 GitHub / npmjs 链接 + 单行描述（溢出渐隐）+「查看完整描述」。
  - 分类标签栏（全部 / 各分类），分类多时右侧出现 ‹ › 滚动控件。
  - 带 `featured`（推荐）或 `same-author`（与本软件同作者）标签的条目排最前并显示 ⭐。
  - 每条提供 GitHub / npmjs 跳转（走 `open_url` 命令，Tauri WebView 下 `target="_blank"` 不生效）；包不在 npm 上时（`noNpm`）不渲染 npmjs 链接，避免 404。
  - 支持 `minDshVersion` 声明版本门槛；已安装的插件标「已安装」并禁用安装按钮。
  - **数据三层结构**：编译期内置 → 磁盘缓存 `<数据根>/cache/plugins.json` → 远端。打开推荐页先读本地立即渲染，再后台检查远端；内容不同（sha256 比对）则写盘并刷新。检查有 30 分钟节流，另有常驻 ↻ 按钮可强制检查。

### 插件路径设置入口

- **新增 `--open-settings=<section>` 参数**：主窗口就绪后滚动到对应设置区块，目前支持 `paths`。
  - 单实例回调识别该参数：已运行则聚焦主窗口并通知前端滚动；未运行则正常启动后滚动。
  - 与 `--launch-version` 同样处理「setup 早于前端挂载」的问题——状态存成可查询的全局变量，前端首帧主动 `take_open_settings` 取走（取后即清，避免重复跳转）。
- **dsh 插件面板新增「打开 multiver 路径设置」按钮**：路径配置异常时用户不必自己找。旧版 multiver 会忽略该参数、只打开主界面，功能降级但不报错。

### 验证

- **单元测试 177 passed / 1 ignored**（新增 `net` 7 条、`catalog` 5 条）。
- **插件单元测试 49 passed**（新增 25 条：镜像链排序、响应形状校验、进度百分比、catalog 校验、版本比较）。
- `cargo fmt` / `cargo clippy -D warnings` 全绿。

---

## [0.3.1] - 2026-10-29

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

## [0.3.0] - 2026-10-28

> **破坏性变更**：CLI 从 `--flag` 风格改为子命令风格。旧语法已移除，迁移说明已归档，不再随仓库发布。
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
- 新增 CLI 迁移指南（历史资料，已归档）。
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
