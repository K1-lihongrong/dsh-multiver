# End-to-end update pipeline smoke test.
#
# It copies the release exe to .tmp, points DSH_MULTIVER_UPDATE_API_BASE at a
# local mock server, runs `update --json`, and verifies:
#   download -> SHA256 -> replace -> .old backup -> delayed relaunch.
#
# The GUI button that calls perform_update is covered by Playwright E2E.
# This script covers the real Rust updater and exe replacement path.

[CmdletBinding()]
param(
    [string]$BaseExe = '',
    [string]$PayloadExe = '',
    [string]$OutputDir = '',
    [string]$MockTag = 'v0.4.1',
    [int]$TimeoutSec = 120,
    [switch]$StopExisting
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'acceptance-common.ps1')

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $PayloadExe) {
    $PayloadExe = Join-Path $PSScriptRoot '..\..\.cuckoo\archive\releases\dsh-multiver-alpha.2.exe'
}
if (-not $BaseExe) {
    $builtExe = Join-Path $repoRoot 'src-tauri\target\release\dsh-multiver.exe'
    if (Test-Path -LiteralPath $builtExe) {
        $BaseExe = $builtExe
    }
    else {
        $BaseExe = 'E:\.LLM-AGENTS\deepseek-harness\dsh-multiver.exe'
    }
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $repoRoot '.tmp\acceptance-update'
}
if (-not (Test-Path -LiteralPath $BaseExe)) { throw "Base exe not found: $BaseExe" }
if (-not (Test-Path -LiteralPath $PayloadExe)) { throw "Payload exe not found: $PayloadExe" }

$runStamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$workRoot = Join-Path $repoRoot ('.tmp\acceptance-update-' + $runStamp)
$appDir = Join-Path $workRoot 'app'
$null = New-Item -ItemType Directory -Path $appDir -Force
$tempExe = Join-Path $appDir 'dsh-multiver.exe'
$oldExe = Join-Path $appDir 'dsh-multiver.exe.old'
Copy-Item -LiteralPath $BaseExe -Destination $tempExe -Force

$baseHash = Get-FileSha256 -Path $BaseExe
$payloadHash = Get-FileSha256 -Path $PayloadExe
$serverProcess = $null
$updateProcess = $null
$newAppProcess = $null
$oldDshHome = $env:DSH_MULTIVER_UPDATE_API_BASE

$result = [ordered]@{
    name = 'acceptance-update'
    ok = $false
    started_at = (Get-Date).ToString('o')
    finished_at = $null
    base_exe = $BaseExe
    payload_exe = $PayloadExe
    work_root = $workRoot
    temp_exe = $tempExe
    base_sha256 = $baseHash
    payload_sha256 = $payloadHash
    replaced_sha256 = $null
    old_backup_sha256 = $null
    update_exit_code = $null
    update_output_ok = $false
    update_stdout = ''
    update_stderr = ''
    old_process_exited = $false
    new_process_started = $false
    new_process_pid = $null
    post_update_version = $null
    maintenance_log_matched = $false
    error = ''
}

$exitCode = 1
try {
    $existing = @(Get-DshProcesses)
    if ($existing.Count -gt 0) {
        if (-not $StopExisting) {
            throw "dsh-multiver.exe is already running. Close it first or rerun with -StopExisting. PIDs: $($existing.ProcessId -join ', ')"
        }
        foreach ($proc in $existing) {
            Stop-DshProcessTree -RootPid ([int]$proc.ProcessId)
        }
        Start-Sleep -Seconds 2
    }

    $node = Get-Command node.exe -ErrorAction Stop
    $serverScript = Join-Path $PSScriptRoot 'mock-update-server.mjs'
    $portFile = Join-Path $workRoot 'server.port'
    $serverLog = Join-Path $workRoot 'server.log'
    $serverErr = Join-Path $workRoot 'server.log.err'
    $serverProcess = Start-Process -FilePath $node.Source `
        -ArgumentList @($serverScript, $PayloadExe, $portFile, $MockTag) `
        -WorkingDirectory $workRoot `
        -RedirectStandardOutput $serverLog -RedirectStandardError $serverErr `
        -PassThru -WindowStyle Hidden

    $deadline = (Get-Date).AddSeconds(15)
    while (-not (Test-Path -LiteralPath $portFile) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 200
    }
    if (-not (Test-Path -LiteralPath $portFile)) {
        throw "Mock update server did not start: $(Get-Content $serverErr -Raw -ErrorAction SilentlyContinue)"
    }
    $port = (Get-Content -LiteralPath $portFile -Raw).Trim()
    $env:DSH_MULTIVER_UPDATE_API_BASE = "http://127.0.0.1:$port"
    Write-Host "Mock update API: $env:DSH_MULTIVER_UPDATE_API_BASE"

    $updateOut = Join-Path $workRoot 'update.stdout.log'
    $updateErr = Join-Path $workRoot 'update.stderr.log'
    $updateProcess = Start-Process -FilePath $tempExe `
        -ArgumentList @('update', '--json') `
        -WorkingDirectory $appDir `
        -RedirectStandardOutput $updateOut -RedirectStandardError $updateErr `
        -PassThru -WindowStyle Hidden

    $updatePid = [int]$updateProcess.Id
    if (-not $updateProcess.WaitForExit($TimeoutSec * 1000)) {
        Stop-DshProcessTree -RootPid $updatePid
        throw "Update process timed out after $TimeoutSec seconds"
    }
    $result.update_exit_code = $updateProcess.ExitCode
    $result.update_stdout = if (Test-Path -LiteralPath $updateOut) { (Read-TextShared -Path $updateOut) } else { '' }
    $result.update_stderr = if (Test-Path -LiteralPath $updateErr) { (Read-TextShared -Path $updateErr) } else { '' }
    $result.update_output_ok = ($result.update_stdout -match '"ok"\s*:\s*true')
    if ($result.update_output_ok -and $null -eq $result.update_exit_code) { $result.update_exit_code = 0 }
    $result.old_process_exited = $true

    # The relaunched app cleans the .old backup during startup. Capture it
    # before the delayed relaunch completes.
    $backupDeadline = (Get-Date).AddSeconds(4)
    while (-not (Test-Path -LiteralPath $oldExe) -and (Get-Date) -lt $backupDeadline) {
        Start-Sleep -Milliseconds 50
    }
    $result.old_backup_sha256 = Get-FileSha256 -Path $oldExe
    Write-Host "Update process exited: $($updateProcess.ExitCode)"
    $deadline = (Get-Date).AddSeconds(30)
    do {
        $newAppProcess = @(
            Get-CimInstance Win32_Process -Filter "Name='dsh-multiver.exe'" -ErrorAction SilentlyContinue |
                Where-Object {
                    $_.ProcessId -ne $updatePid -and
                    $_.ExecutablePath -and
                    ([IO.Path]::GetFullPath($_.ExecutablePath) -eq [IO.Path]::GetFullPath($tempExe))
                } |
                Select-Object -First 1
        )
        if ($newAppProcess.Count -gt 0) { break }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)

    if ($newAppProcess.Count -eq 0) {
        throw 'Updated exe did not relaunch within 30 seconds'
    }
    $result.new_process_started = $true
    $result.new_process_pid = [int]$newAppProcess[0].ProcessId
    Write-Host "Relaunched pid=$($result.new_process_pid)"

    Start-Sleep -Seconds 2
    $result.replaced_sha256 = Get-FileSha256 -Path $tempExe

    $maintenanceLog = Join-Path $appDir 'logs\maintenance.log'
    if (Test-Path -LiteralPath $maintenanceLog) {
        $match = Select-String -LiteralPath $maintenanceLog -Pattern '新版本已替换|已安排重启' -Encoding utf8 -ErrorAction SilentlyContinue
        $result.maintenance_log_matched = ($null -ne $match)
    }

    Write-Host 'checkpoint: stopping relaunched app'
    Stop-DshProcessTree -RootPid ([int]$newAppProcess[0].ProcessId)
    Start-Sleep -Seconds 2

    Write-Host 'checkpoint: version command'
    $versionText = (& $tempExe version --json 2>&1 | Out-String)
    if ($versionText -match '"version"\s*:\s*"([^"]+)"') {
        $result.post_update_version = $Matches[1]
    }
    Write-Host "Post-update version: $($result.post_update_version)"
    Write-Host 'checkpoint: checks'

    $checks = @(
        $result.update_output_ok,
        $result.old_process_exited,
        $result.new_process_started,
        ($result.replaced_sha256 -eq $payloadHash),
        ($result.replaced_sha256 -ne $baseHash),
        ($result.old_backup_sha256 -eq $baseHash),
        ($null -ne $result.post_update_version),
        ($result.post_update_version -ne '0.4.0')
    )
    $result.ok = -not ($checks -contains $false)
}
catch {
    $result.error = ("" + $_).Trim()
    Write-Warning $result.error
}
finally {
    if ($null -ne $newAppProcess -and $newAppProcess.Count -gt 0) {
        try { Stop-DshProcessTree -RootPid ([int]$newAppProcess[0].ProcessId) } catch {}
    }
    if ($null -ne $serverProcess) {
        Write-Host 'checkpoint: stopping mock server'
        Stop-Process -Id ([int]$serverProcess.Id) -Force -ErrorAction SilentlyContinue
    }
    if ($null -eq $oldDshHome) {
        Remove-Item Env:DSH_MULTIVER_UPDATE_API_BASE -ErrorAction SilentlyContinue
    }
    else {
        $env:DSH_MULTIVER_UPDATE_API_BASE = $oldDshHome
    }

    $result.finished_at = (Get-Date).ToString('o')
    if ($result.ok) { $exitCode = 0 } else { $exitCode = 1 }
    Write-Host 'checkpoint: writing report'
    if (-not (Test-Path -LiteralPath $OutputDir)) { $null = New-Item -ItemType Directory -Path $OutputDir -Force }
    $textPath = Join-Path $OutputDir ('acceptance-update-' + $runStamp + '.txt')
    $textLines = @()
    foreach ($entry in $result.GetEnumerator()) { $textLines += ('' + $entry.Key + ': ' + $entry.Value) }
    [IO.File]::WriteAllText($textPath, ($textLines -join [Environment]::NewLine), (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "REPORT TEXT: $textPath"
    Write-Host "ACCEPTANCE UPDATE: $(if ($result.ok) { 'PASS' } else { 'FAIL' })"
}

exit $exitCode
