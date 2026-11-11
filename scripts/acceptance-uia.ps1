# Native UI acceptance for a packaged dsh-multiver.exe.
# This drives the real Tauri window and real Rust backend.
#
# Typical use:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/acceptance-uia.ps1 -StopExisting
#
# Default data root is the local acceptance harness:
#   E:\.LLM-AGENTS\deepseek-harness

[CmdletBinding()]
param(
    [string]$ExePath = 'E:\.LLM-AGENTS\deepseek-harness\dsh-multiver.exe',
    [string]$WorkingDirectory = 'E:\.LLM-AGENTS\deepseek-harness',
    [string]$ExpectedVersion = '0.4.0-alpha.3',
    [string]$LaunchVersion = '0.2.2-rc.1',
    [string]$OutputDir = '',
    [int]$StartupTimeoutSec = 30,
    [int]$LaunchTimeoutSec = 120,
    [switch]$SkipPlugin,
    [switch]$SkipScan,
    [switch]$SkipLaunch,
    [switch]$StopExisting,
    [switch]$KeepAppOpen,
    [switch]$NoRestoreConfig
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'acceptance-common.ps1')

if (-not $OutputDir) {
    $OutputDir = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\.tmp\acceptance-uia'))
}
if (-not (Test-Path -LiteralPath $OutputDir)) {
    $null = New-Item -ItemType Directory -Path $OutputDir -Force
}
if (-not (Test-Path -LiteralPath $ExePath)) {
    throw "Exe not found: $ExePath"
}

$runStamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$result = [ordered]@{
    name = 'acceptance-uia'
    ok = $false
    started_at = (Get-Date).ToString('o')
    finished_at = $null
    exe = $ExePath
    working_directory = $WorkingDirectory
    expected_version = $ExpectedVersion
    actual_version = $null
    installed_versions = @()
    plugin_panel = $null
    scan_started = $null
    scan_cancelled = $null
    launch_version = $null
    launch_window = $null
    launch_ready = $null
    launch_closed = $null
    residual_processes = @()
    log_errors = @()
    screenshots = @()
    config_sha256_before = $null
    config_sha256_after = $null
    config_restored = $null
    error = ''
}

$appProcess = $null
$mainWindow = $null
$logDir = Join-Path $WorkingDirectory 'logs'
$logNames = @('app.log', 'frontend.log', 'maintenance.log')
$logOffsets = @{}
foreach ($name in $logNames) {
    $path = Join-Path $logDir $name
    if (Test-Path -LiteralPath $path) {
        $logOffsets[$name] = (Get-Item -LiteralPath $path).Length
    }
    else {
        $logOffsets[$name] = 0
    }
}

$nodeBaseline = @()
if (-not $SkipLaunch) {
    $nodeBaseline = @(
        Get-CimInstance Win32_Process -Filter "Name='node.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -match [regex]::Escape($LaunchVersion) } |
            Select-Object -ExpandProperty ProcessId
    )
}

$configPath = Join-Path $WorkingDirectory 'config.json'
$configBackup = Join-Path $OutputDir 'config.before.json'
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

    $configPath = Join-Path $WorkingDirectory 'config.json'
    $configBackup = Join-Path $OutputDir 'config.before.json'
    if (Test-Path -LiteralPath $configPath) {
        Copy-Item -LiteralPath $configPath -Destination $configBackup -Force
        $result.config_sha256_before = Get-FileSha256 -Path $configPath
    }
    Write-Host "Starting: $ExePath"
    $appProcess = Start-Process -FilePath $ExePath -WorkingDirectory $WorkingDirectory -PassThru
    $mainWindow = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern '^DSH 版本管理器$' -TimeoutSec $StartupTimeoutSec
    Write-Host "Main window: $($mainWindow.Current.Name)"

    $versionEl = Wait-DshElement -Window $mainWindow -ControlType Text -NameRegex '^v\d+\.\d+\.\d+' -TimeoutSec 20
    $result.actual_version = $versionEl.Current.Name
    Write-Host "Version: $($result.actual_version)"

    $all = Get-DshElements -Window $mainWindow
    $installedSet = @{}
    for ($i = 0; $i -lt $all.Count; $i++) {
        try {
            $el = $all[$i]
            if ($el.Current.ControlType.ProgrammaticName -eq 'ControlType.Text' -and $el.Current.Name -match '^\d+\.\d+\.\d+') {
                $installedSet[$el.Current.Name] = $true
            }
        }
        catch {}
    }
    $installed = @($installedSet.Keys | Sort-Object)
    $result.installed_versions = $installed
    if ($installed.Count -eq 0) {
        throw 'No installed versions found in the main window'
    }
    Write-Host "Installed versions: $($installed.Count)"
    if (-not $SkipPlugin) {
        Write-Host 'Checking plugin panel'
        $pluginButton = Find-DshElement -Window $mainWindow -ControlType Button -Name '插件' -EnabledOnly
        if ($null -eq $pluginButton) { throw 'Plugin button not found' }
        $null = Invoke-DshElement -Element $pluginButton

        $null = Wait-DshElement -Window $mainWindow -ControlType TabItem -Name '已安装' -TimeoutSec 15
        $installedItems = @(
            Get-DshElements -Window $mainWindow |
                Where-Object {
                    try { $_.Current.ControlType.ProgrammaticName -eq 'ControlType.ListItem' -and $_.Current.Name -match '已安装' } catch { $false }
                }
        )
        $result.plugin_panel = ($installedItems.Count -gt 0)
        $shot = Save-DshScreenshot -Window $mainWindow -Path (Join-Path $OutputDir 'plugin-panel.png')
        $result.screenshots += $shot

        $closeButton = Find-DshElement -Window $mainWindow -ControlType Button -Name '✕'
        if ($null -ne $closeButton) {
            $null = Invoke-DshElement -Element $closeButton
        }
        else {
            $null = Invoke-DshElement -Element $pluginButton -Collapse
        }
        $null = Wait-DshCondition -TimeoutSec 10 -Message 'plugin panel close' -Condition {
            $null -eq (Find-DshElement -Window $mainWindow -ControlType TabItem -Name '推荐')
        }
        Write-Host "Plugin panel: $($result.plugin_panel)"
    }

    if (-not $SkipScan) {
        Write-Host 'Checking scan progress and cancellation'
        $scanButton = Find-DshElement -Window $mainWindow -ControlType Button -Name '扫描全部占用' -EnabledOnly
        if ($null -eq $scanButton) { throw 'Scan all button not found' }
        $null = Invoke-DshElement -Element $scanButton
        $null = Wait-DshElement -Window $mainWindow -ControlType Button -Name '取消扫描' -TimeoutSec 10
        $result.scan_started = $true
        $shot = Save-DshScreenshot -Window $mainWindow -Path (Join-Path $OutputDir 'scan-progress.png')
        $result.screenshots += $shot

        $cancelButton = Find-DshElement -Window $mainWindow -ControlType Button -Name '取消扫描' -EnabledOnly
        if ($null -eq $cancelButton) { throw 'Cancel scan button not found' }
        $null = Invoke-DshElement -Element $cancelButton
        $result.scan_cancelled = Wait-DshCondition -TimeoutSec 20 -Message 'scan cancellation' -Condition {
            $cancel = Find-DshElement -Window $mainWindow -ControlType Button -Name '取消扫描'
            $idle = Find-DshElement -Window $mainWindow -ControlType Button -Name '扫描全部占用'
            ($null -eq $cancel) -and ($null -ne $idle)
        }
        $shot = Save-DshScreenshot -Window $mainWindow -Path (Join-Path $OutputDir 'scan-cancelled.png')
        $result.screenshots += $shot
        Write-Host "Scan started/cancelled: $($result.scan_started)/$($result.scan_cancelled)"
    }
    if (-not $SkipLaunch) {
        Write-Host "Checking launch: $LaunchVersion"
        $result.launch_version = $LaunchVersion

        $versionPattern = '^' + [regex]::Escape($LaunchVersion)
        $row = $null
        foreach ($el in (Get-DshElements -Window $mainWindow)) {
            try {
                if ($el.Current.ControlType.ProgrammaticName -eq 'ControlType.ListItem' -and $el.Current.Name -match $versionPattern) {
                    $row = $el
                    break
                }
            }
            catch {}
        }
        if ($null -eq $row) { throw "Version row not found: $LaunchVersion" }
        try { $row.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView() } catch {}
        Start-Sleep -Milliseconds 800

        $versionText = Wait-DshElement -Window $mainWindow -ControlType Text -Name $LaunchVersion -TimeoutSec 10
        $rowY = $versionText.Current.BoundingRectangle.Y
        $runButton = $null
        foreach ($el in (Get-DshElements -Window $mainWindow)) {
            try {
                if ($el.Current.ControlType.ProgrammaticName -eq 'ControlType.Button' -and
                    $el.Current.Name -eq '运行' -and $el.Current.IsEnabled -and
                    [math]::Abs($el.Current.BoundingRectangle.Y - $rowY) -lt 35) {
                    $runButton = $el
                    break
                }
            }
            catch {}
        }
        if ($null -eq $runButton) { throw "Run button not found for $LaunchVersion" }
        $frontendLog = Join-Path $logDir 'frontend.log'
        $frontendOffset = if (Test-Path -LiteralPath $frontendLog) { (Get-Item -LiteralPath $frontendLog).Length } else { 0 }
        $null = Invoke-DshElement -Element $runButton

        $launchTitle = '^DSH\s+' + [regex]::Escape($LaunchVersion) + '\s*$'
        $dshWindow = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern $launchTitle -TimeoutSec $LaunchTimeoutSec
        $result.launch_window = $true
        $readyPattern = '\[topbar\] injected ver=' + [regex]::Escape($LaunchVersion)
        $result.launch_ready = Wait-DshCondition -TimeoutSec $LaunchTimeoutSec -Message "launch ready: $LaunchVersion" -Condition {
            $appended = Read-AppendedText -Path $frontendLog -Offset $frontendOffset
            $appended -match $readyPattern
        }
        Start-Sleep -Seconds 2
        Write-Host "Launch ready: $($result.launch_ready)"
        try {
            $shot = Save-DshScreenshot -Window $dshWindow -Path (Join-Path $OutputDir 'launch-window.png')
            $result.screenshots += $shot
        }
        catch {}

        try {
            $dshWindow.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        }
        catch {
            throw "Could not close launched window: $($_.Exception.Message)"
        }
        $null = Wait-DshCondition -TimeoutSec 20 -Message 'launched window close' -Condition {
            $remainingWindows = @(
                Get-DshTopWindows -ProcessId $appProcess.Id |
                    Where-Object {
                        try { $_.Current.Name -match $launchTitle } catch { $false }
                    }
            )
            $remainingWindows.Count -eq 0
        }
        $result.launch_closed = $true

        $newNodePids = @(
            Get-CimInstance Win32_Process -Filter "Name='node.exe'" -ErrorAction SilentlyContinue |
                Where-Object {
                    $_.CommandLine -match [regex]::Escape($LaunchVersion) -and
                    -not ($nodeBaseline -contains $_.ProcessId)
                } |
                Select-Object -ExpandProperty ProcessId
        )
        if ($newNodePids.Count -gt 0) {
            try {
                $null = Wait-DshCondition -TimeoutSec 20 -Message 'launch child process cleanup' -Condition {
                    $remaining = @(
                        Get-CimInstance Win32_Process -Filter "Name='node.exe'" -ErrorAction SilentlyContinue |
                            Where-Object { $newNodePids -contains $_.ProcessId }
                    )
                    $remaining.Count -eq 0
                }
            }
            catch {
                Write-Warning $_.Exception.Message
            }
        }
        Write-Host "Launch window/ready/closed: $($result.launch_window)/$($result.launch_ready)/$($result.launch_closed)"
    }
}
catch {
    $result.error = ("" + $_).Trim()
    Write-Warning $result.error
}
finally {
    $logErrors = @()
    foreach ($name in $logNames) {
        $path = Join-Path $logDir $name
        $appended = Read-AppendedText -Path $path -Offset $logOffsets[$name]
        if ($appended -match '(?i)panic|panicked|unhandled exception|unhandled promise rejection|fatal error') {
            $logErrors += [pscustomobject]@{ file = $name; text = $appended }
        }
    }
    $result.log_errors = $logErrors

    if ($null -ne $mainWindow -and -not $KeepAppOpen) {
        try {
            $mainWindow.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        }
        catch {}
        Start-Sleep -Seconds 2
    }
    if ($null -ne $appProcess -and -not $KeepAppOpen) {
        try {
            $appProcess.Refresh()
            if (-not $appProcess.HasExited) {
                Stop-DshProcessTree -RootPid ([int]$appProcess.Id)
            }
        }
        catch {}
        Start-Sleep -Seconds 1
    }

    if (-not $KeepAppOpen -and -not $NoRestoreConfig -and (Test-Path -LiteralPath $configBackup)) {
        Copy-Item -LiteralPath $configBackup -Destination $configPath -Force
    }
    $result.config_sha256_after = Get-FileSha256 -Path $configPath
    if ($null -eq $result.config_sha256_before) {
        $result.config_restored = $true
    }
    else {
        $result.config_restored = ($result.config_sha256_after -eq $result.config_sha256_before)
    }

    if (-not $KeepAppOpen) {
        $residual = @()
        foreach ($proc in (Get-DshProcesses)) {
            $live = Get-Process -Id ([int]$proc.ProcessId) -ErrorAction SilentlyContinue
            if ($null -ne $live -and -not $live.HasExited) {
                $residual += [pscustomobject]@{
                    pid = [int]$proc.ProcessId
                    name = $proc.Name
                    command_line = $proc.CommandLine
                }
            }
        }
        $result.residual_processes = $residual
    }

    $actualNormalized = ([string]$result.actual_version).TrimStart('v', 'V')
    $expectedNormalized = ([string]$ExpectedVersion).TrimStart('v', 'V')
    $checks = @(
        ($actualNormalized -eq $expectedNormalized),
        ($result.installed_versions.Count -gt 0)
    )
    if (-not $SkipPlugin) { $checks += [bool]$result.plugin_panel }
    if (-not $SkipScan) { $checks += [bool]$result.scan_started; $checks += [bool]$result.scan_cancelled }
    if (-not $SkipLaunch) { $checks += [bool]$result.launch_window; $checks += [bool]$result.launch_ready; $checks += [bool]$result.launch_closed }
    $checks += ($result.log_errors.Count -eq 0)
    if (-not $KeepAppOpen) { $checks += ($result.residual_processes.Count -eq 0) }

    if (-not $KeepAppOpen) { $checks += [bool]$result.config_restored }
    $result.ok = -not ($checks -contains $false)
    $result.finished_at = (Get-Date).ToString('o')
    if ($result.ok) { $exitCode = 0 }
    else { $exitCode = 1 }

    Write-AcceptanceReport -Result $result -OutputDir $OutputDir -Name ('acceptance-uia-' + $runStamp)
    Write-Host "ACCEPTANCE UI: $(if ($result.ok) { 'PASS' } else { 'FAIL' })"
}

exit $exitCode
