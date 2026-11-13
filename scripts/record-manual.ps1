param(
  [string[]]$Lang = @("zh", "en", "ja", "ko", "zh-TW"),
  [string[]]$Only = @()
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$ui = @{
  "zh" = "zh-CN"; "en" = "en-US"; "ja" = "ja-JP"; "ko" = "ko-KR"; "zh-TW" = "zh-TW"
}

Push-Location $root
try {
  foreach ($l in $Lang) {
    if (-not $ui.ContainsKey($l)) { throw "未知语言：$l（可用：zh,en,ja,ko,zh-TW）" }
    $env:REC_LANG = $ui[$l]
    Write-Host "=== 录制 $l（界面 $($ui[$l])）===" -ForegroundColor Cyan

    $playArgs = @("exec", "playwright", "test", "--config=playwright.record.config.js")
    if ($Only.Count -gt 0) { $playArgs += @("--grep", ($Only -join "|")) }
    & pnpm @playArgs
    if ($LASTEXITCODE -ne 0) { throw "Playwright 失败：$l（exit $LASTEXITCODE）" }

    & node e2e/record/build-gifs.mjs "--lang=$l"
    if ($LASTEXITCODE -ne 0) { throw "转码失败：$l（exit $LASTEXITCODE）" }
  }
}
finally {
  Remove-Item Env:REC_LANG -ErrorAction SilentlyContinue
  Pop-Location
}
