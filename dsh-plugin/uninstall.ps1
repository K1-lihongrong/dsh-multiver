# 卸载 dsh-multiver-bridge
param(
  [Parameter(Mandatory = $true)][string]$DshCmd,
  [string]$Profile = 'mvtest2',
  [string]$DshHome = $env:DSH_HOME
)

$ErrorActionPreference = 'Stop'
if (-not $DshHome) { throw 'DSH_HOME 未设置，请传入 -DshHome' }
if (-not (Test-Path $DshCmd)) { throw "找不到 dsh.cmd：$DshCmd" }
$env:DSH_HOME = $DshHome

& $DshCmd plugin --profile $Profile remove dsh-multiver-bridge
if ($LASTEXITCODE -ne 0) {
  throw "卸载失败（exit $LASTEXITCODE）"
}
Write-Host "已卸载。请重启 profile。"
