# 安装 dsh-multiver-bridge 到指定 profile
param(
  [Parameter(Mandatory = $true)][string]$DshCmd,
  [string]$Profile = 'mvtest2',
  [string]$PackageDir = $PSScriptRoot,
  [string]$DshHome = $env:DSH_HOME,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
if (-not $DshHome) { throw 'DSH_HOME 未设置，请传入 -DshHome' }
if (-not (Test-Path $DshCmd)) { throw "找不到 dsh.cmd：$DshCmd" }
$env:DSH_HOME = $DshHome

function Invoke-Dsh {
  param([string[]]$Arguments)
  & $DshCmd @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "dsh 命令失败（exit $LASTEXITCODE）：$($Arguments -join ' ')"
  }
}

$profileDir = Join-Path $DshHome "profiles\$Profile"
$manifestPath = Join-Path $profileDir 'package.json'

if (-not (Test-Path $manifestPath)) {
  Write-Host "profile $Profile 不存在，从 web 模板创建..."
  Invoke-Dsh @('--profile', $Profile, '--from-default-profile', 'web', '--dump-config') | Out-Null
} else {
  $bundles = (Get-Content $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json).dsh.profile.bundles
  if ($bundles -notcontains '@deepseek-ai/dsh-web-app') {
    if ($Force) {
      Write-Warning "profile '$Profile' 缺 @deepseek-ai/dsh-web-app，-Force 已跳过检查，继续安装。"
      Write-Warning "该 profile 的 Web UI 可能无法启动。补法：& '$DshCmd' plugin --profile $Profile add @deepseek-ai/dsh-web-app"
    } else {
      throw "profile '$Profile' 缺 @deepseek-ai/dsh-web-app，Web UI 将无法启动。先补该 bundle，或加 -Force 强制继续。"
    }
  }
}

Write-Host "安装插件：$PackageDir"
Invoke-Dsh @('plugin', '--profile', $Profile, 'add', $PackageDir)
Write-Host "完成。请重启 profile 生效。"
