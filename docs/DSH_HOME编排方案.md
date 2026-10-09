# DSH_HOME 编排方案（复用官方配置）

> 编写日期：2026-10-09 · 基线：v0.2.5
> 定位：允许用户**复用官方 `.dsh` 配置目录**，一个开关、默认关、随时可退、不丢数据。
> 关联：[开发缺口](开发缺口.md)（本方案对应 Backlog 新条目）

---

## 一、要解决的问题

管理器当前把非隔离版本的 `DSH_HOME` 一律指到 `<数据根>/home`，用户从"裸装 dsh"迁过来时，
**已有的 `~/.dsh`（会话 / 凭据 / 插件）全部不可见**，像换了个新软件。这是迁移用户最大的摩擦点。

需求：**让用户能复用官方默认的 `.dsh`**，且方案要简洁、不复杂。

---

## 二、官方默认目录（背景知识）

dsh 作为 Node 应用，官方默认 home：

| 平台 | 默认路径 |
| :--- | :--- |
| Windows | `%USERPROFILE%\.dsh` |
| macOS / Linux | `~/.dsh` |

`DSH_HOME` 环境变量优先；未设置时 dsh 自身有路径回退链（启动器旁 `.dsh` → npm 全局 → `~/.dsh`）。
**dsh 不排斥被显式指向 `~/.dsh`**，第三方封装只是默认隔离。

---

## 三、方案（简洁版）

**只加一个布尔字段、一个分支、一个开关。**

### 3.1 配置字段

```rust
// config.rs
pub use_official_dsh_home: bool,   // 默认 false（保持现有行为）
```

### 3.2 解析逻辑

```rust
fn resolve_home(cfg, version) -> PathBuf {
    if cfg.is_isolated(version) {
        return versions_dir.join(version).join("home");   // 隔离：不变
    }
    if cfg.use_official_dsh_home {
        return official_dsh_home();                        // 新增：~/.dsh
    }
    dirs.home                                              // 默认：不变
}

fn official_dsh_home() -> Option<PathBuf> {
    #[cfg(windows)] std::env::var("USERPROFILE").ok().map(|p| Path::new(&p).join(".dsh"))
    #[cfg(unix)]    dirs::home_dir().map(|p| p.join(".dsh"))
}
```

### 3.3 优先级

隔离 > 复用官方 > 管理器共享。**隔离开关永远优先**（版本级 > 全局级）。

---

## 四、UI

路径设置区加一行：

```
[ ] 复用官方 dsh 配置目录（~/.dsh）
    开启后直接使用你原有的会话、凭据和插件。
```

**一个复选框，一句说明。**

---

## 五、防呆（只做必要的两条）

### 1. 首次开启时弹一次确认

```
复用官方配置后，管理器将读取 %USERPROFILE%\.dsh。
你原有的数据不会删除，随时可关闭此选项恢复。

[取消]  [开启]
```

一句话讲清：**不会删数据、可恢复**。

### 2. 目录不存在时不静默

开启后若 `~/.dsh` 不存在：

```
⚠️ 未检测到 %USERPROFILE%\.dsh

dsh 将自动创建该目录，你的现有数据不受影响。

[知道了]
```

---

## 六、不做的事

- ❌ 不做三档模式
- ❌ 不做每版本独立 home 来源
- ❌ 不做首次启动智能引导
- ❌ 不做数据量对比预览
- ❌ 不做自动迁移
- ❌ 不做 home 来源标签（版本卡片保持干净）

---

## 七、一致性

`build_forward_script()` 读同一个 `resolve_home()`，模式切换后重新生成 `dsh.cmd` / `~/.local/bin/dsh`。
**GUI 和终端行为永远一致**，无需额外设计。

---

## 八、改动清单

| 文件 | 改动 |
| :--- | :--- |
| `config.rs` | 加字段 `use_official_dsh_home`（默认 false）+ 序列化 |
| `lib.rs` / 命令层 | `resolve_home()` 加分支；加 `official_dsh_home()` |
| `commands.rs` | 加 `set_use_official_home(bool)` 命令；切换后重生成转发脚本 |
| `cli.rs` | 转发脚本的 DSH_HOME 走同一逻辑 |
| `App.vue` | 路径设置区加复选框 + 首次开启确认 / 目录不存在提示 |

**成本**：约 1 小时。

---

## 九、为什么这样够了

用户的需求本质是一句话：**"我想让 dsh 还认识我原来的配置。"**

一个开关就能回答。默认关，不打扰现有用户；想复用的人勾一下，**立即生效、随时可退、不丢数据**。

复杂方案解决的是"用户可能不知道怎么选"的问题——但这里只有一个开关、一句人话说明，
用户不会不知道怎么选。
