## DSH 版本管理器 v0.3.2

本版解决两个实际障碍：**无代理用户更新不了**，以及**找不到可装的插件**。

### 🚀 下载走镜像回退

此前所有依赖 GitHub 的下载（自身更新、插件部署、插件暂存更新）都只有单源直连。装代理的用户靠系统代理能过，没装的用户直连超时——更新功能形同虚设。

现在按序尝试多个源，任一成功即返回：

| 用途 | 源顺序 |
| :--- | :--- |
| exe 下载 | 直连 `github.com` → `gh-proxy.com` |
| 推荐列表 | 直连 → `gh-proxy.com` → `cdn.jsdelivr.net` |

- 直连优先，官方源可用时不引入第三方。
- 失败判定不只看状态码——公共反代常返回 200 但内容是 HTML 错误页，因此下完后还要校验响应形状（exe 检查 `MZ`/`ELF` 头，JSON 检查首字符）。
- 镜像列表硬编码，不需要用户配置环境变量。

实测（厦门，无代理）：`github.com` 的 Release 资产 2.1 秒可达，但 `raw.githubusercontent.com` 10 秒超时；`gh-proxy.com` 代理同一资产 1.6 秒。

### 🧩 推荐插件页

插件面板新增「已安装 | 推荐」分段切换。

- **分类筛选**：顶部标签栏按分类过滤，分类多时右侧出现 `‹ ›` 滚动控件。
- **单网格自适应**：与已安装列表一致，宽屏多列、窄屏单列。
- **卡片双行**：第一行包名 + 徽标 + 安装按钮；第二行 GitHub / npmjs 链接 + 单行描述。描述过长右侧渐隐，悬停或点「查看完整描述」看全文。
- **星标置顶**：带 `featured`（推荐）或 `same-author`（与本软件同作者）标签的条目排最前并显示 ⭐。
- **一键安装**：复用已安装标签的安装对话框，已装的条目禁用安装按钮。

**列表数据分三层**：

1. 编译期内置 —— 首次打开时的初始内容
2. 磁盘缓存 `<数据根>/cache/plugins.json` —— 上次成功拉取的内容
3. 远端 `main` 分支 —— 每次检查时的比对源

打开推荐页先读本地**立即渲染**，再后台检查远端；内容不同（sha256 比对）则写盘并自动刷新。检查有 30 分钟节流，分类栏右侧另有常驻 ↻ 按钮可强制检查。断网时用上次缓存或内置副本，顶部提示「可能不是最新」，功能不受影响。

**新增插件不用发版**：改 `docs/plugins.json` 推送到 main 即可，用户下次打开推荐页就能看到。

### 🛠️ dsh 插件 0.2.1

- **下载进度**：部署向导与暂存更新显示百分比、已下载字节数、已用时长，不再是「处理中…」干等。
- **下载可取消**：下载中途可中止，临时文件自动清理。
- **更新期间明确提示**：multiver 的 `update` 是阻塞子进程，中途终止会留下半替换的 exe，因此改为明确标注「更新期间不可取消」并显示已用时长。
- **新增「打开 multiver 路径设置」按钮**：路径配置异常时不必自己找。

安装（装完需重启 dsh）：

```powershell
# 方式一：npm
dsh plugin --profile web add dsh-multiver-bridge

# 方式二：GitHub Release
dsh plugin --profile web add https://github.com/K1-lihongrong/dsh-multiver/releases/download/v0.3.2/dsh-multiver-bridge-0.2.1.tgz
```

### 🔧 其他改进

- **新增 `--open-settings=<section>` 参数**：主窗口就绪后滚动到对应设置区块，目前支持 `paths`。
- **外部链接修正**：GitHub / npmjs 此前用 `<a target="_blank">`，在 Tauri WebView 里不触发系统浏览器；改为走 `open_url` 命令。

### 📖 文档

- [交互设计约定](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/交互设计约定.md) 新增 2.9 节「推荐插件页」，含三层数据、浮窗触发方式、星标语义的设计理由。
- [使用手册](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/使用手册.md) 插件章节补「推荐」标签用法。

### ⚠️ 已知限制

- 推荐列表停留在页面上超过 30 分钟不会自动刷新，需点 ↻ 或重启程序。
- 断网用户看到的是随程序发布的内置副本，可能与远端不一致。
- Linux 侧代码已实现，真桌面验收后置到 v0.3.x。
- macOS 未适配（需真机）。

### 平台

Windows（主，已本机验收）、Linux（AppImage / deb，真桌面验收后置）。

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
