# App.vue 拆分计划（GAP-009）

> **状态：✅ 已完成（2026-10-05）。App.vue 1336 → 173 行。**
> 实际拆分出：NotesPanel / EnvCheck / IsolatedMenu / InstalledList / RemoteList /
> PathSettings 六个组件，useToast / useSharedHomeWarn / notesCache 三个 composable，
> 以及全局样式 `src/styles/common.css`。
> 说明：第 8 步实际做的是"样式收拢到 common.css"（评估后认为抽 useManagerState 收益小、
> 改动面大，未做）。下面保留原始计划以备查。

---

> 目标：把 `src/App.vue`（当时 **1133 行**）按职责拆成多个组件 + composable，
> **分步实施、每步可验证**，避免一次性大重构的风险。
>
> 背景：本项目前端**无自动化测试**，靠"小步 + 手动验证"控制风险。每步拆完都要
> `pnpm build` + 真机点一遍相关功能。
>
> 登记：GAP-009（见 `开发缺口.md`）

---

## 一、App.vue 现状结构（1133 行）

| 区块 | 行数（约） | 说明 |
| :--- | :--- | :--- |
| `<script setup>` | ~620 | 全部状态 + 逻辑 |
| `<template>` | ~350 | 全部界面 |
| `<style>` | ~160 | 全部样式（非 scoped） |

**script 里的逻辑分组**（按功能）：

1. **环境自检**：`envChecks` / `envChecked` / `envPassed` / `runEnvCheck`
2. **已安装版本列表**：`installed` / `verSizes` / `scanVersionSize` / `run` / `uninstall` / `setDefault` / `createShortcut` / `openInBrowser` / `toggleIsolated`
3. **隔离管理菜单**：`openMenu` / `scanning` / `sizes` / `toggleMenu` / `scanSize` / `copyShared` / `clearIsolated` / `openIsolatedDir`
4. **批量操作**：`selected` / `batchBusy` / `toggleSelect` / `toggleSelectAll` / `batchUninstall` / `isBlocked`
5. **可用版本 + 安装**：`remote` / `filteredRemote` / `loadRemote` / `pickVersion` / `install` / `doInstall` / `retryInstall` / `classifyError` / `isBroken` / `clearBroken`
6. **安装进度**：`installStage` / `installStep` / `STAGE_START` / `progressPct` + `install-progress` 监听
7. **路径设置**：`rootInput` / `applyRoot` / `resetRoot` / `openDir` / `openUrl` / `setWarnSharedHome`
8. **维护面板**：`doMaintenance` / `toggleAutoMaint` / `fmtTs` / `state.maintenance`
9. **更新说明**：`remoteNoteVer` / `installedNoteVer` + `<NotesPanel>` ✅（已拆）
10. **通用**：`state` / `notify` / `toast` / `refresh` / 生命周期 / 错误上报

---

## 二、拆分原则

1. **一次只拆一个**，拆完立即验证（`pnpm build` + 真机点相关功能）。
2. **优先拆"低耦合、边界清晰"的**——先易后难。
3. **状态提升 vs 下放**：能自包含的（自己拉数据、自己管状态）就下放到组件；跨组件共享的留在 App.vue 或抽 composable。
4. **不追求"零 App.vue"**：App.vue 保留"布局 + 共享状态 + 事件编排"是合理的；目标是**降到 400-500 行**。
5. **样式**：拆出的组件用 `<style scoped>`；App.vue 的非 scoped 样式逐步收窄。

---

## 三、分步计划

### ✅ 第 1 步（已完成）：`NotesPanel.vue`
- 抽出「版本更新说明」面板（自包含：拉取 + 消毒 + 渲染）
- App.vue 只留 `remoteNoteVer` / `installedNoteVer` 两个状态
- 成果：App.vue 1336 → 1133 行

### 第 2 步：`EnvCheck.vue`（环境自检面板）
- 抽出：`envChecks` / `envChecked` / `envPassed` / `runEnvCheck` + 模板里的环境检查区
- 自包含：组件自己调 `check_env`、自己管状态
- 对外：暴露 `passed`（安装前检查需要）——可用 `defineExpose` 或让父组件读
- 预计减少：~80 行

### 第 3 步：`useToast`（composable，非组件）
- 抽出：`toast` / `toastExpanded` / `notify` / `dismissToast` / `toggleToast` / `toastTimer`
- 所有组件/逻辑都要用 `notify`，适合做成 composable（`src/composables/useToast.js`）
- 对外：`{ toast, toastExpanded, notify, dismissToast, toggleToast }`
- 预计减少：~40 行

### 第 4 步：`IsolatedMenu.vue`（隔离管理下拉菜单）
- 抽出：`openMenu` / `scanning` / `sizes` / `toggleMenu` / `scanSize` / `copyShared` / `clearIsolated` / `openIsolatedDir` + 菜单模板
- 需要传 `version` / `isolated`，emit 必要事件
- 预计减少：~90 行

### 第 5 步：`RemoteList.vue`（可用版本 + 安装）
- 抽出：`remote` / `filteredRemote` / `loadRemote` / `pickVersion` / `install` / `doInstall` / `retryInstall` / `classifyError` / `isBroken` / `clearBroken` + 可用版本模板 + 换源重试弹窗
- 依赖：`refresh`（安装后刷新列表）、`notify`（用 useToast）
- 这是**较大的一块**，拆完 App.vue 明显变小
- 预计减少：~250 行

### 第 6 步：`InstalledList.vue`（已安装版本列表 + 批量操作）
- 抽出：`installed` / `verSizes` / `scanVersionSize` / `run` / `uninstall` / `setDefault` / `createShortcut` / `openInBrowser` / `toggleIsolated` / 批量操作相关 + 已安装列表模板
- 依赖：`confirmSharedHome`、`openMenu` 状态（与 IsolatedMenu 协作）
- 预计减少：~250 行

### 第 7 步：`PathSettings.vue` + `MaintenancePanel.vue`
- 抽出路径设置、维护面板
- 预计减少：~120 行

### 第 8 步（收尾）：抽取共享状态到 composable
- `useManagerState`：`state` / `refresh` / `rootInput` 等跨组件共享的状态
- App.vue 只留布局 + 编排

---

## 四、每步的验证清单

拆任何一步后，**必须手动点一遍**：
- [ ] 环境自检显示正常
- [ ] 已安装版本列表正常（运行/卸载/设为默认/扫描/隔离菜单）
- [ ] 可用版本列表正常（刷新/搜索/安装/换源）
- [ ] 安装进度条正常
- [ ] 路径设置正常
- [ ] 维护面板正常
- [ ] 更新说明展开正常
- [ ] 无控制台报错（`pnpm build` 无警告）

---

## 五、风险与注意

- **共享状态是主要难点**：`state` / `installed` / `notify` 被多处引用，拆时要理清"谁拥有、谁消费"。
- **事件编排**：安装成功后要 `refresh()`，这类跨组件联动用 emit 向上传或 composable 共享。
- **样式**：App.vue 当前是非 scoped 全局样式，拆出组件用 scoped 时要确认样式不丢。
- **不引入状态库**：保持"Vue 原生 + composable"，不引入 Pinia（与项目"极简"风格一致）。
