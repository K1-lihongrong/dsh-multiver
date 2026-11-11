# CLI 迁移指南：v0.4.0 实例模型

> 适用：从 v0.3.x 升级到 v0.4.0 的脚本/自动化。
> 结论：**旧命令仍然可用**；`isolate` 的 `--off` 语义从「关闭隔离」变为「解除该版本全部实例挂载」，实例数据保留。

## 1. 兼容行为

| 旧用法 | v0.4.0 行为 |
| :--- | :--- |
| `isolate <版本>` | 幂等确保：已挂载则保留挂载与显式主按钮目标；主按钮为共享时指向首实例；未挂载则建「默认隔离」并挂载 |
| `isolate <版本> --off` | 解除该版本全部实例挂载；实例本体与 `homes/<id>` 保留，可在 `instance list` 看到孤儿 |
| `run <版本>` | 使用该版本的 `selected_target`（实例或共享）启动 |
| `copy-shared <版本>` | 只接受实例目标；共享目标会提示用 `--instance` |
| `clear-isolated <版本>` | 同上，必须指向实例 home |

> 升级后第一次运行会自动迁移 `isolated_versions`：每个隔离版本生成一个「默认隔离」实例，主按钮指向它；旧数据从 `versions/<版本>/home` 移到 `homes/<id>`。

## 2. 新命令

```bash
instance create --name <名称> [--attach <版本>...] [--json]
instance list [--json]
instance rename <id> <新名>
instance remove <id>

isolate <版本> --list [--json]
isolate <版本> --attach <id>
isolate <版本> --detach <id>

run <版本> --instance <id>
copy-shared <版本> --instance <id>
clear-isolated <版本> --instance <id>
```

- `instance create` 不创建目录；首次运行创建 home。
- 名称全局唯一（不区分大小写）、≤32 字符、不能含 `/` `\` 或控制字符。
- `instance remove` 运行中拒绝；home 移入 `trash/`，由后台/下次启动清理。

## 3. JSON 字段变化

`isolate <版本> --list --json`：

```json
{
  "version": "0.2.1-rc.4",
  "isolated": true,
  "selected_target": "i2",
  "instances": [
    { "id": "i1", "name": "默认隔离", "home": ".../homes/i1", "mounted_versions": ["0.2.1-rc.4"], "orphan": false, "running_on": null, "size": null }
  ]
}
```

`instance list --json`：`{ "instances": [...], "version_targets": { "版本": "实例id 或 null" } }`。

`run <版本> --json` 新增 `instance` 字段；`info <版本> --json` 新增 `selected_target` 与 `instances`。

## 4. 备份兼容

- 新备份包含实例表；`import` 整体恢复。
- 旧备份（只有 `isolated_versions`）导入时会为每个隔离版本合成「默认隔离」实例；不搬 home 数据，版本需另行安装。

## 5. Linux

v0.4.0 明确**不打包、不发布 Linux 产物**。Linux 只保留 CI 编译/测试；实例功能未在真桌面验收（见 GAP-020），未来恢复 Linux 发布时再补。
