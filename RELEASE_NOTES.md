## DSH 版本管理器 v0.3.1

本版聚焦 **dsh 插件（dsh-multiver-bridge）**：从「只能看版本」扩展为「能部署、能更新、能自助安装」。

### 🚀 新增 CLI 命令

```bash
dsh-multiver set-root <目录>     # 设置数据根目录（绝对路径）
```

- 支持 `--dry-run` 预览、`--json` 机读输出（含 `root` / `previous` / `changed` / `created`）。
- 会创建子目录并重生成终端转发脚本，与界面「路径设置」完全等价。
- Linux 上切换前会清理旧数据根的残留 dsh 进程登记。

详见 [CLI 迁移指南 · 第七节](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/CLI迁移指南-v0.3.0.md#七v031-新增命令无需迁移)。

### 🧩 dsh 插件 0.2.0

在 dsh 设置页提供「版本管理」，读的是同一个数据根，与主程序界面一致。

**安装**（装完需重启 dsh）：

```powershell
# 方式一：npm
dsh plugin --profile web add dsh-multiver-bridge

# 方式二：GitHub Release
dsh plugin --profile web add https://github.com/K1-lihongrong/dsh-multiver/releases/download/v0.3.1/dsh-multiver-bridge-0.2.0.tgz
```

**本版新增能力**：

| 能力 | 说明 |
| :--- | :--- |
| **部署引导** | 首次使用且未配置数据根时，只显示三步向导：选工作目录 → 获取 exe（自动下载 / 使用已有）→ 设置数据根并启动。下载走 GitHub Release 并校验 SHA256。 |
| **更新管理** | 页面底部可检查并更新 multiver 自身。全部委托 multiver 自带 updater，插件不重复实现替换逻辑。 |
| **内嵌窗口也能更新** | 在内嵌窗口点更新会**暂存**新版本（下载 + 校验 + 写标记），multiver **下次启动时**自动完成替换。当前会话完全不受影响。 |
| **兼容性提示** | 运行版本不在已验证列表时显示黄色提示，但**不阻断功能**。已验证：`0.1.5-rc.3` / `0.2.0-rc.1` / `0.2.0-rc.2` / `0.2.1-rc.2` / `0.2.1-rc.3`。 |

**安装层零版本约束**：不声明 `engines.dsh`，任何 dsh 版本都能装。最低已验证版本 `0.1.5-rc.3`，完整矩阵见 [插件兼容矩阵](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/插件兼容矩阵.md)。

### 🔧 其他改进

- **插件面板搜索框**：从顶部工具栏移到列表上方，加放大镜图标，文案改为「筛选已安装的插件…」，避免与「添加插件」按钮混淆。
- **设置页导航图标**：第三方插件此前只能显示齿轮（与「通用设置」重复），现改为按标签定位并替换为专属图标。
- **空版本态文案**：明确 multiver 只管理自己装在数据根 `versions/` 下的 dsh，通过 npm / pnpm / Python wheel / 桌面版安装的 dsh 是独立的一份。
- **部署下载前检查磁盘空间**：不足 100 MB 直接拒绝，不开始下载。
- **`set-root --json` 补 `created` 字段**：区分「目录是否新建」与「根是否变化」。

### 📖 文档

- 新增 [插件兼容矩阵](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/插件兼容矩阵.md)：8 个 dsh 版本的实测结果与失败原因分析。
- [使用手册](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/使用手册.md) 补「与其它方式装的 dsh 共存」一节：npm / pnpm / Python wheel / 桌面版四种形态的冲突情况与判断方法。

### ⚠️ 已知限制

- 部署下载与更新**没有进度显示与取消能力**，见 [开发缺口 GAP-016](https://github.com/K1-lihongrong/dsh-multiver/blob/main/docs/开发缺口.md)。
- Linux 侧代码已实现，真桌面验收后置到 v0.3.x。
- macOS 未适配（需真机）。

### 平台

Windows（主，已本机验收）、Linux（AppImage / deb，真桌面验收后置）。

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
