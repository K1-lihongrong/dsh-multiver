# dsh 双向桥 —— 调研结论与交接文档

> 编写日期：2026-10-20 · 基线：v0.3.0（CLI 已定稿）
> 目标：**dsh（DeepSeek Harness）界面 ⇄ dsh-multiver 双向联动**。
> 本文是**交给 dsh 开发方**的交接文档：含调研结论、功能拆解、API 用法、验收标准。

---

## 一、背景

dsh-multiver 是 dsh 的多版本管理器（独立桌面工具，Tauri + Rust）。CLI 已于 v0.3.0 定稿为子命令风格（`dsh-multiver list` / `install X` / `set-default X` …）。

**双向桥的设想**：

1. **dsh 界面内** → 调 dsh-multiver CLI → 管理版本（列/切/装）
2. **dsh-multiver 界面内** → 为 dsh 安装/调试插件

---

## 二、调研结论（已验证）

### 2.1 dsh 插件机制：成熟可用

**证据**（来自 `@deepseek-ai/dsh` 0.2.1-rc.2 实际安装）：

- **`dsh plugin add/remove/list`**：插件 = npm 包，装在 profile 目录（**转发给 pnpm**）。
- **官方自带 60+ 个 `@deepseek-ai/dsh-client-ui-*` 插件**（agent-preset / model-selection / open-in-app …）——**插件机制是官方主推的扩展方式**。
- **cordis 框架**（`@deepseek-ai/cordis` ~4.0.5）+ `dsh-client-ui-cordis`（客户端 UI 插件运行器）。
- 配套：`dsh-plugin-manager`（插件管理）、`dsh-host-*`（Host 端能力）、`dsh-api-*`（API 网关）。

### 2.2 插件结构（标准模板）

```
@deepseek-ai/dsh-client-ui-xxx/
├── package.json          # 含 dsh.client.inject 声明
├── lib/
│   ├── index.js          # Host 端插件（apply()，可为空）
│   ├── client.js         # 浏览器端插件（UI）
│   └── types/*.d.ts
└── locale/{zh,en}.json   # 文案
```

**`package.json` 关键字段**（以 `dsh-client-ui-open-in-app` 为例）：

```json
{
  "name": "@deepseek-ai/dsh-client-ui-open-in-app",
  "type": "module",
  "exports": {
    ".": { "default": "./lib/index.js" },
    "./client": { "default": "./lib/client.js" }
  },
  "dsh": {
    "client": {
      "inject": [
        "@deepseek-ai/dsh-api-gateway",
        "@deepseek-ai/dsh-client-locale",
        "@deepseek-ai/dsh-client-ui-layout",
        "@deepseek-ai/dsh-client-ui-renderer",
        "@deepseek-ai/dsh-client-ui-slots"
      ],
      "platform": "web"
    }
  },
  "peerDependencies": { "@deepseek-ai/cordis": "~4.0.5" }
}
```

### 2.3 客户端插件 API（lib/client.js）

```js
window.__ModuleLoader__.load({
  id: "@deepseek-ai/dsh-client-ui-xxx",
  factory: (require) => {
    var module = { exports: {} };
    function apply(ctx) {
      // 1) 注册 UI（注入到指定 slot）
      ctx.slots.inject("conversation.session.header.utilities", () =>
        ctx.slots.register({ id: "my-entry" })
      );
      // 2) 注册文案
      ctx.effect(() => ctx.locale.register(NS, { "key": "文案" }));
      // 3) 调后端 API
      ctx.remote.someMethod();
      // 4) 订阅状态
      ctx.sessions.list.getSnapshot();
    }
    module.exports = { apply };
    return module.exports;
  },
});
```

### 2.4 可用 slot 位置（注入 UI 的挂载点）

已验证存在：
- `conversation.session.header.utilities` —— 会话头部工具区（**适合放"版本管理"按钮**）
- `sidebar.right.tab.files.actions` —— 右侧文件面板操作区
- `sidebar.right.tab.document.actions` —— 右侧文档面板操作区

### 2.5 Host 端调外部命令

- `@deepseek-ai/dsh-native-command`（`runner` / `path-opener`）—— 执行系统命令
- `@deepseek-ai/dsh-subprocess` —— 子进程管理

**这是"dsh 调 dsh-multiver CLI"的技术基础**。

---

## 三、功能拆解（可独立实现，建议按序）

### B1. dsh-multiver 侧：插件管理界面（**低风险，独立**）

**做什么**：dsh-multiver 界面加"dsh 插件"面板，管理**已安装 dsh 版本**的插件。

**怎么调**：
```bash
dsh plugin list                         # 列已装插件
dsh plugin add <包名>                   # 安装
dsh plugin remove <包名>                # 卸载
dsh plugin add <包名> --profile <版本>  # 指定 profile
```

**依赖**：无（纯 CLI 调用）。
**归属**：**dsh-multiver 侧开发**（不需要 dsh 插件）。

### B2. dsh 插件骨架（**中风险，需要 dsh 侧**）

**做什么**：创建 `dsh-client-ui-multiver` npm 包，**空插件**（能装、能在 dsh 里注册、不报错）。

**参考**：`dsh-client-ui-open-in-app` 的完整结构。

**验收**：`dsh plugin add <包>` 后 dsh 正常启动，插件出现在插件列表。

### B3. 插件内加"版本管理"入口（**高风险**）

**做什么**：在 `conversation.session.header.utilities` 注入按钮/面板：
- 显示 dsh-multiver 管理的版本列表
- 点击可切换默认版本

**怎么调 dsh-multiver**：Host 端用 `dsh-native-command` 执行：
```bash
dsh-multiver list --json
dsh-multiver which --json
dsh-multiver set-default X
```

**难点**：
- 需理解 cordis `ctx` 完整 API（slots / locale / remote / effect）。
- 需 Host 端（`lib/index.js`）实现命令执行 + 结果返回。
- 无公开文档，**需读官方插件源码**。

### B4. 双向联动（**高风险，可选**）

- dsh-multiver 界面显示"当前 dsh 正在用什么版本"
- dsh 内切换版本 → dsh-multiver 界面同步

**依赖**：B1+B2+B3 全部完成。

---

## 四、验收标准

| 项 | 标准 |
| :--- | :--- |
| **B1** | dsh-multiver 界面能列/装/卸 dsh 插件；调 `dsh plugin` 输出正确展示 |
| **B2** | 空插件能通过 `dsh plugin add` 安装、dsh 启动正常 |
| **B3** | dsh 界面出现"版本管理"入口；能列出版本、切换默认；错误有提示 |
| **B4** | 双界面状态同步 |

**通用**：不影响 dsh 原有功能；插件可卸载干净；i18n 中英。

---

## 五、参考代码（本地路径）

`E:\.LLM-AGENTS\deepseek-harness\versions\0.2.1-rc.2\node_modules\.pnpm\`

**最有参考价值的插件**：
- `@deepseek-ai+dsh-client-ui-_01bc45015b358938c5a4476776845193` → `dsh-client-ui-open-in-app`
  - 完整示例：Host 端（空 apply）+ Client 端（slots 注入 + locale + remote）
  - `lib/client.js`（896 行）**是最好的学习材料**

**关键包**：
- `@deepseek-ai/cordis`（框架）
- `@deepseek-ai/dsh-client-ui-slots`（UI 挂载点）
- `@deepseek-ai/dsh-native-command`（调外部命令）
- `@deepseek-ai/dsh-api-gateway` / `dsh-api-remotes`（前后端通信）
- `@deepseek-ai/dsh-plugin-manager`（插件管理）

**官方仓库**：https://github.com/deepseek-ai/deepseek-harness（`packages/client/ui-*` 目录）

---

## 六、给 dsh 开发方的建议

1. **先做 B2（骨架）**：证明插件能跑通，再往里加功能。
2. **以 `dsh-client-ui-open-in-app` 为模板**：它同时演示了 slots 注入、locale、remote 调用、Host 端空实现。
3. **Host 端调 CLI** 用 `dsh-native-command` 的 `runner`。
4. **UI 只放在 slot 里**（不要改 dsh 核心），保证可卸载。
5. **dsh-multiver CLI 已稳定**（v0.3.0 子命令风格），可直接依赖。

---

## 七、分工建议

| 块 | 归属 |
| :--- | :--- |
| **B1**（multiver 插件管理界面） | **dsh-multiver 自己**（我可做） |
| **B2-B4**（dsh 插件） | **dsh 侧**（交接给 dsh 开发方） |
