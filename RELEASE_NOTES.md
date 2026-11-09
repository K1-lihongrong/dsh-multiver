# v0.3.7

本次为**安全与契约加固版**：修复 v0.3.6 内嵌 dsh 窗口事件桥的来源校验缺失，拆分 capability，合并启动意图状态，并补上 i18n、外部契约与自更新失败路径测试。无用户可见交互变化。

---

## 内嵌窗口事件监听按窗口绑定

v0.3.6 用全局 `app.listen_any` 接收 topbar 事件，并信任 payload 里的 `version` 字段。任意 dsh 页面（含第三方插件代码）都能伪造版本号，关闭或重启其他版本的窗口。

- 删除 4 个全局 topbar 监听器，改到 `launch_window` 内用 `window.listen` 绑定当前窗口。
- 版本号与进程映射由闭包捕获，payload 里的 `version` 不再被采信。
- topbar 从全局 `emit` 改为 `emit_to` 当前窗口 label，避免广播到所有窗口。
- 每个窗口注入随机 128-bit token；四个监听器都校验 token，页面内代码无法读取其他窗口的闭包 token，跨窗口伪造事件被拒绝。
- 监听器在窗口显示前注册，避免页面首帧事件早于监听。

**用户可见变化**：无。两版本并行时，关闭/重启/主题切换只作用于当前窗口。

## capability 拆两份

原 `default.json` 同时覆盖主窗口和 `dsh-*`，把 `core:default`、`opener`、`dialog` 与窗口三键全部授给加载第三方内容的窗口。

- 拆为 `capabilities/main.json`（主窗口）与 `capabilities/dsh.json`（内嵌窗口），删除合并版。
- 内嵌窗口只保留 `core:event:allow-emit-to` 与 `core:window:allow-start-dragging`。
- 三键/重启/主题由 Rust 侧处理，不需要给 dsh 页面 JS 窗口权限；`opener` / `dialog` / `core:default` 已移除。

**用户可见变化**：无。内嵌窗口拖拽、三键、重启、主题跟随保持可用。

## LaunchIntent 合并启动状态

把 `LAUNCH_MODE` / `LAUNCH_ERROR` / `OPEN_SETTINGS` 三个 static 合并为 `LaunchIntent`，由一把锁管理三个字段。

- 写入/读取时机与语义不变，只减少重复状态与 lock 函数。
- `window.rs` 重建主窗口时同时清空 `mode` 与 `error`。

**用户可见变化**：无。

## i18n key 校验

新增 `scripts/check-i18n.mjs`，以 zh-CN 为基准检查其余 4 种语言 key 是否齐全；CI 在 frontend build 后执行。

- 当前 5 种语言均为 361 key。
- 缺 key 时运行期会静默回退中文，构建不会报错，因此必须由 CI 拦截。

**用户可见变化**：无。

## 契约测试（A5）

新增 `src-tauri/tests/contracts.rs` 与独立 CI job：

- fake pnpm 输出驱动生产 `ProgressParser`，锁住阶段、明细与 fraction。
- pnpm 错误文本分类对拍。
- `pnpm view --json` 三种输出形态解析对拍。
- 真实 GBK 字节解码。
- 真实 pnpm view 联网对拍（`#[ignore]`，独立 job 显式运行）。

**用户可见变化**：无。

## staged 自更新失败路径测试（A6）

抽出 `take_over_pending_at` 以注入被替换 exe 路径，新增 `src-tauri/tests/staged_failures.rs`：

- 目标 exe 缺失时中止并保留唯一可用的 `.old`。
- target rename 失败时保留原 exe、暂存与 pending 标记。
- 放入新 exe 失败时回滚旧 exe。
- 成功路径消费暂存与 pending 标记。

**用户可见变化**：无。

---

## 验证

- Rust 单测 **202 passed / 1 ignored**
- staged 失败路径 **4 passed**
- 契约测试 **4 passed / 1 ignored**；真实 pnpm 对拍 **1 passed**
- `cargo clippy --all-targets -- -D warnings` 0 warning；`cargo fmt --check` 通过
- `pnpm build` 通过；5 语 i18n 各 361 key 全绿
- `pnpm tauri build --no-bundle` 本地打包通过
- 实机并行两个版本（0.2.2-rc.1 / 0.2.2-alpha.3）：
  - 最小化 rc.1 后 alpha.3 仍正常；
  - 关闭 alpha.3 后 rc.1 窗口仍在、进程未退出；
  - 「重启」后窗口不消失、dsh node PID 更换、topbar 重新注入；
  - 最小化 / 最大化 / 还原 / 关闭三键均实测可用；
  - 标题栏拖拽实测可用；
  - 主题切换实测：标题栏背景 `rgb(247,247,248)` ↔ `rgb(31,35,40)`，DWM 深色属性 20 在 0/1 间同步。
- 主题跟随建议验收时切换 dsh 深浅色再确认一次（事件通道与窗口权限已随三键/重启一并验证）。
