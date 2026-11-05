<#
tmp-clean.ps1 —— 清空 dsh-multiver/.tmp/ 内的临时产物

安全约束（见 .cuckoo/rules/00-协作约定 §9）：
  - 目标路径写死，不接受参数覆盖
  - 执行前校验父目录是仓库根（含 .gitignore）
  - 只清空 .tmp/ 内的条目，不删 .tmp/ 目录本身
  - 默认移入回收站，-Permanent 才彻底删除

用法：
  powershell -File scripts/tmp-clean.ps1            # 移入回收站
  powershell -File scripts/tmp-clean.ps1 -Permanent # 彻底删除
#>
param(
    [switch]$Permanent
)

$ErrorActionPreference = 'Stop'

# 目标目录写死：脚本所在目录的上一级（仓库根）下的 .tmp
$repoRoot = Split-Path -Parent $PSScriptRoot
$target = Join-Path $repoRoot '.tmp'

# 校验 1：仓库根必须含 .gitignore（防止脚本被搬到别处运行）
if (-not (Test-Path (Join-Path $repoRoot '.gitignore') -PathType Leaf)) {
    Write-Error "拒绝执行：$repoRoot 不是仓库根（缺少 .gitignore）"
    exit 1
}

# 校验 2：目标目录名必须正好是 .tmp（防路径拼错）
if ((Split-Path -Leaf $target) -ne '.tmp') {
    Write-Error "拒绝执行：目标目录不是 .tmp（实际：$target）"
    exit 1
}

if (-not (Test-Path $target -PathType Container)) {
    Write-Output "目录不存在，无需清理：$target"
    exit 0
}

$items = Get-ChildItem -LiteralPath $target -Force
if ($items.Count -eq 0) {
    Write-Output "目录已空：$target"
    exit 0
}

if ($Permanent) {
    foreach ($item in $items) {
        Remove-Item -LiteralPath $item.FullName -Recurse -Force
    }
    Write-Output ("已彻底删除 " + $items.Count + " 项（" + $target + "）")
} else {
    Add-Type -AssemblyName Microsoft.VisualBasic
    $ok = 0
    foreach ($item in $items) {
        if ($item.PSIsContainer) {
            [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory(
                $item.FullName, 'OnlyErrorDialogs', 'SendToRecycleBin')
        } else {
            [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile(
                $item.FullName, 'OnlyErrorDialogs', 'SendToRecycleBin')
        }
        $ok++
    }
    Write-Output ("已移入回收站 " + $ok + " 项（" + $target + "）")
}
