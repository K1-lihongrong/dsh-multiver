//! 帮助文本（v0.3.0 子命令风格）。

use super::VERSION;

pub fn print_help() {
    println!(
        "dsh-multiver {} —— DSH 多版本管理器（无头 CLI）

用法：
  dsh-multiver <命令> [参数...] [修饰符...]

命令：
  list [--json]                    列出已安装版本（默认版本以 \"* \" 前缀标记）
  which [--json]                   输出默认版本 dsh 入口的绝对路径（无默认版本则失败）
  info <版本> [--json]             查看单个版本详情（路径 / 默认 / 隔离 / 配置目录 / 占用）
  versions [--json]                列出远端可用版本（从 npm 查询，最新在前）
  env [--json]                     环境检查（有致命项失败时退出码 1）
  doctor [--json]                  一键诊断（目录 / 失败标记 / 已安装 / 环境）

  install <版本> [--registry <url>]  安装指定版本（--registry 可指定 npm 源）
  uninstall <版本>                 卸载指定版本
  set-default <版本>               设为默认版本，并重生成终端 dsh 命令
  isolate <版本> [--off]           开启某版本的数据隔离（加 --off 则关闭）
  shortcut <版本>                  为某版本创建桌面快捷方式
  copy-shared <版本>               把共享 home 数据复制到某隔离版本的独立 home（需先开隔离）
  clear-isolated <版本>            清空某隔离版本的独立 home（保留版本本身）
  run <版本>                       启动 dsh web（新控制台 + 自动打开浏览器；关窗口即停止）
  export [<文件>]                  导出配置 + 版本清单（JSON）；默认写到数据根目录
  import <文件>                    导入配置（从 export 生成的备份文件）
  clean                            清理孤立缓存 + 回收站（maintenance --cleanup 的超集）
  maintenance [--cleanup | --prune] 磁盘维护：清孤立缓存 / 回收依赖仓库 / 不带则两者都做
  setup-runtime [--dry-run]        检测并下载缺失的便携运行时（Node/pnpm）
  update [--dry-run]               检查并执行自动更新（下载 → 校验 → 替换 → 重启）

  help                             显示本帮助
  version [--json]                 显示版本号

修饰符：
  --json       结构化输出（机读）；只影响格式，不改行为
  --dry-run    只预览不执行（可搭配 install / uninstall / set-default / isolate /
               clean / maintenance / import / shortcut / copy-shared / clear-isolated /
               setup-runtime / update）
  --force      覆盖非本管理器生成的 dsh 命令（用于 set-default）
  --registry <url>  指定 npm 源（用于 install）
  --off        关闭隔离（用于 isolate）

示例：
  dsh-multiver list
  dsh-multiver install 0.1.7
  dsh-multiver set-default 0.1.7
  dsh-multiver list --json
  DSH=\"$(dsh-multiver which)\"

  不带任何命令时，启动图形界面。
  退出码：成功 0 / 失败 1。",
        VERSION
    );
}
