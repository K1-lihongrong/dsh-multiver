#requires -Version 5.1
<#
.SYNOPSIS
  Native multi-instance acceptance for dsh-multiver.

.DESCRIPTION
  Exercises same-version instances through the real manager UI:
  create two disposable instances, launch both, close A and verify B,
  close the manager and verify child cleanup, then restart once.

  This script mutates config.json and creates temporary instance homes.
  It snapshots config.json and removes the test instances in finally.
#>
[CmdletBinding()]
param(
    [string]$ExePath = 'E:\.LLM-AGENTS\deepseek-harness\dsh-multiver.exe',
    [string]$WorkingDirectory = 'E:\.LLM-AGENTS\deepseek-harness',
    [string]$Version = '0.2.2-rc.1',
    [string]$OutputDir = '',
    [string[]]$InstanceNames = @(),
    [switch]$StopExisting,
    [switch]$NoRestoreConfig,
    [switch]$KeepTestInstances,
    [string[]]$MatrixVersions = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'acceptance-common.ps1')
Add-Type -AssemblyName System.Windows.Forms

# Avoid depending on script file encoding under Windows PowerShell 5.1.
$script:InstanceMenuName = ([string]([char]0x5B9E) + [char]0x4F8B + [char]0x7BA1 + [char]0x7406)
$script:InstanceRunLabel = ([string]([char]0x8FD0) + [char]0x884C)
$script:InstanceFocusLabel = ([string]([char]0x805A) + [char]0x7126)
$script:InstanceRunRegex = ('^(' + $script:InstanceRunLabel + '|' + $script:InstanceFocusLabel + ')')

if (-not $OutputDir) {
    $OutputDir = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\.tmp\acceptance-instances'))
}
if (-not (Test-Path -LiteralPath $OutputDir)) {
    $null = New-Item -ItemType Directory -Path $OutputDir -Force
}
if (-not (Test-Path -LiteralPath $ExePath)) { throw "Exe not found: $ExePath" }
if (-not (Test-Path -LiteralPath $WorkingDirectory)) { throw "Working directory not found: $WorkingDirectory" }

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if ($InstanceNames.Count -eq 0) {
    $InstanceNames = @("accA-$stamp", "accB-$stamp")
}
if ($InstanceNames.Count -lt 2) { throw 'InstanceNames needs at least two names' }
$configPath = Join-Path $WorkingDirectory 'config.json'
$configBackup = Join-Path $OutputDir 'config.before.json'
$createdIds = @()
$appProcess = $null
$result = [ordered]@{
    name = 'acceptance-instances'
    ok = $false
    started_at = (Get-Date).ToString('o')
    finished_at = $null
    exe = $ExePath
    working_directory = $WorkingDirectory
    version = $Version
    instance_names = $InstanceNames
    instance_ids = @()
    launched = @()
    close_a_kept_b = $false
    manager_exit_clean = $false
    restart_ok = $false
    residual_processes = @()
    screenshots = @()
    error = ''
}

function Invoke-CliJson {
    param([string[]]$Arguments)
    $out = Join-Path $OutputDir ("cli-" + [guid]::NewGuid().ToString('N') + ".out")
    $err = Join-Path $OutputDir ("cli-" + [guid]::NewGuid().ToString('N') + ".err")
    $proc = Start-Process -FilePath $ExePath -ArgumentList $Arguments -WorkingDirectory $WorkingDirectory -Wait -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
    $stdout = if (Test-Path -LiteralPath $out) { Get-Content -LiteralPath $out -Raw } else { '' }
    $stderr = if (Test-Path -LiteralPath $err) { Get-Content -LiteralPath $err -Raw } else { '' }
    if ($proc.ExitCode -ne 0) {
        throw ("CLI failed ($($proc.ExitCode)): " + ($Arguments -join ' ') + [Environment]::NewLine + $stdout + [Environment]::NewLine + $stderr)
    }
    if ($stdout.Trim()) { return ($stdout | ConvertFrom-Json) }
    return $null
}

function Get-VersionRow {
    param([System.Windows.Automation.AutomationElement]$Window, [string]$VersionText)
    $deadline = (Get-Date).AddSeconds(30)
    do {
        $all = Get-DshElements -Window $Window
        foreach ($el in $all) {
            try {
                if ($el.Current.ControlType.ProgrammaticName -eq 'ControlType.ListItem' -and $el.Current.Name -match ('^' + [regex]::Escape($VersionText))) {
                    try { $el.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern).ScrollIntoView() } catch {}
                    Start-Sleep -Milliseconds 500
                    return $el
                }
            }
            catch {}
        }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)
    throw "Version row not found: $VersionText"
}

function Get-RowY {
    param([System.Windows.Automation.AutomationElement]$Window, [string]$VersionText)
    $text = Wait-DshElement -Window $Window -ControlType Text -Name $VersionText -TimeoutSec 10
    return $text.Current.BoundingRectangle.Y
}

function Find-ButtonNearRow {
    param(
        [System.Windows.Automation.AutomationElement]$Window,
        [double]$RowY,
        [string]$Name = '',
        [string]$NameRegex = ''
    )
    $best = $null
    $bestDistance = [double]::MaxValue
    foreach ($el in (Get-DshElements -Window $Window)) {
        try {
            if ($el.Current.ControlType.ProgrammaticName -ne 'ControlType.Button') { continue }
            if (-not $el.Current.IsEnabled) { continue }
            $distance = [math]::Abs($el.Current.BoundingRectangle.Y - $RowY)
            if ($distance -gt 60 -or $distance -ge $bestDistance) { continue }
            if ($Name -and $el.Current.Name -ne $Name) { continue }
            if ($NameRegex -and $el.Current.Name -notmatch $NameRegex) { continue }
            $best = $el
            $bestDistance = $distance
        }
        catch {}
    }
    return $best
}

function Find-Radio {
    param([System.Windows.Automation.AutomationElement]$Window, [string]$Name)
    foreach ($el in (Get-DshElements -Window $Window)) {
        try {
            if ($el.Current.ControlType.ProgrammaticName -eq 'ControlType.RadioButton' -and $el.Current.Name -eq $Name) {
                return $el
            }
        }
        catch {}
    }
    return $null
}

function Close-TopWindow {
    param([System.Windows.Automation.AutomationElement]$Window)
    $Window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
}

function Wait-TitleGone {
    param([int]$ProcessId, [string]$TitlePattern, [int]$TimeoutSec = 20)
    return (Wait-DshCondition -TimeoutSec $TimeoutSec -Message "window gone: $TitlePattern" -Condition {
        $rows = @(Get-DshTopWindows -ProcessId $ProcessId | Where-Object {
            try { $_.Current.Name -match $TitlePattern } catch { $false }
        })
        $rows.Count -eq 0
    })
}

function ExistingManagerPids {
    return @(Get-DshProcesses | ForEach-Object { [int]$_.ProcessId })
}

$exitCode = 1
try {
    $existing = @(ExistingManagerPids)
    if ($existing.Count -gt 0) {
        if (-not $StopExisting) { throw "dsh-multiver is already running: $($existing -join ', ')" }
        foreach ($pidToStop in $existing) { Stop-DshProcessTree -RootPid $pidToStop }
        Start-Sleep -Seconds 2
    }

    if (Test-Path -LiteralPath $configPath) {
        Copy-Item -LiteralPath $configPath -Destination $configBackup -Force
    }
    else {
        throw "config.json not found: $configPath"
    }

    $baselineNode = @(
        Get-CimInstance Win32_Process -Filter "Name='node.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -and $_.CommandLine -match [regex]::Escape($Version) } |
            Select-Object -ExpandProperty ProcessId
    )

    Write-Host "Creating disposable instances for $Version"
    foreach ($name in $InstanceNames) {
        $created = Invoke-CliJson -Arguments @('instance', 'create', '--name', $name, '--attach', $Version, '--json')
        if ($null -eq $created -or -not $created.id) { throw "instance create returned no id for $name" }
        $createdIds += [string]$created.id
    }
    $result.instance_ids = @($createdIds)
    Write-Host "Created: $($createdIds -join ', ')"

    Write-Host "Starting manager"
    $appProcess = Start-Process -FilePath $ExePath -WorkingDirectory $WorkingDirectory -PassThru
    $mainWindow = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern '^DSH' -TimeoutSec 30

    foreach ($name in $InstanceNames) {
        Write-Host "Launching instance: $name"
        $row = Get-VersionRow -Window $mainWindow -VersionText $Version
        $rowY = Get-RowY -Window $mainWindow -VersionText $Version
        $caret = Find-ButtonNearRow -Window $mainWindow -RowY $rowY -Name $script:InstanceMenuName
        if ($null -eq $caret) { throw "Instance menu caret not found for $Version" }
        $null = Invoke-DshElement -Element $caret
        Start-Sleep -Milliseconds 600

        $radio = Find-Radio -Window $mainWindow -Name $name
        if ($null -eq $radio) { throw "Instance radio not found: $name" }
        $null = Invoke-DshElement -Element $radio
        Start-Sleep -Milliseconds 500

        $radioY = $radio.Current.BoundingRectangle.Y
        $runButton = Find-ButtonNearRow -Window $mainWindow -RowY $radioY -NameRegex $script:InstanceRunRegex
        if ($null -eq $runButton) {
            [DshAcceptanceWin]::SetForegroundWindow([IntPtr]$mainWindow.Current.NativeWindowHandle) | Out-Null
            [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
            Start-Sleep -Milliseconds 300
            $runButton = Find-ButtonNearRow -Window $mainWindow -RowY $rowY -NameRegex $script:InstanceRunRegex
        }
        if ($null -eq $runButton) {
            $debug = @()
            foreach ($el in (Get-DshElements -Window $mainWindow)) {
                try {
                    if ($el.Current.ControlType.ProgrammaticName -eq 'ControlType.Button') {
                        $debug += ("name=" + $el.Current.Name + " y=" + [int]$el.Current.BoundingRectangle.Y + " enabled=" + $el.Current.IsEnabled)
                    }
                }
                catch {}
            }
            Write-Host ("BUTTONS:" + [Environment]::NewLine + ($debug -join [Environment]::NewLine))
            throw "Run button not found after selecting $name"
        }
        $null = Invoke-DshElement -Element $runButton

        $titlePattern = '^DSH\s+' + [regex]::Escape($Version) + '\s+.+\s+' + [regex]::Escape($name) + '$'
        $dshWindow = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern $titlePattern -TimeoutSec 120
        $result.launched += @{ name = $name; title = $dshWindow.Current.Name }
        Write-Host "Launched: $($dshWindow.Current.Name)"
        $safeName = $name -replace '[^A-Za-z0-9_-]', '_'
        $shot = Save-DshScreenshot -Window $dshWindow -Path (Join-Path $OutputDir ("launch-" + $safeName + ".png"))
        $result.screenshots += $shot
    }

    if ($result.launched.Count -lt 2) { throw 'Expected two launched instances' }

    $titleA = '^DSH\s+' + [regex]::Escape($Version) + '\s+.+\s+' + [regex]::Escape($InstanceNames[0]) + '$'
    $titleB = '^DSH\s+' + [regex]::Escape($Version) + '\s+.+\s+' + [regex]::Escape($InstanceNames[1]) + '$'
    $windowA = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern $titleA -TimeoutSec 20
    $windowB = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern $titleB -TimeoutSec 20

    Write-Host 'Closing instance A'
    Close-TopWindow -Window $windowA
    $null = Wait-TitleGone -ProcessId $appProcess.Id -TitlePattern $titleA -TimeoutSec 20
    $windowB = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern $titleB -TimeoutSec 20
    $result.close_a_kept_b = $true
    Write-Host 'Close A kept B alive'

    Write-Host 'Closing instance B'
    Close-TopWindow -Window $windowB
    $null = Wait-TitleGone -ProcessId $appProcess.Id -TitlePattern $titleB -TimeoutSec 20

    Write-Host 'Closing manager'
    Close-TopWindow -Window $mainWindow
    $null = Wait-DshCondition -TimeoutSec 20 -Message 'manager exit' -Condition {
        $null -eq (Get-Process -Id $appProcess.Id -ErrorAction SilentlyContinue)
    }
    Start-Sleep -Seconds 1

    $newNode = @(
        Get-CimInstance Win32_Process -Filter "Name='node.exe'" -ErrorAction SilentlyContinue |
            Where-Object {
                $_.CommandLine -and $_.CommandLine -match [regex]::Escape($Version) -and
                -not ($baselineNode -contains $_.ProcessId)
            }
    )
    $result.residual_processes = @($newNode | Select-Object -ExpandProperty ProcessId)
    if ($result.residual_processes.Count -gt 0) { throw "Residual node processes: $($result.residual_processes -join ', ')" }
    $result.manager_exit_clean = $true
    Write-Host 'Manager exit left no child processes'

    Write-Host 'Restarting manager'
    $appProcess = Start-Process -FilePath $ExePath -WorkingDirectory $WorkingDirectory -PassThru
    $mainWindow = Wait-DshWindow -ProcessId $appProcess.Id -TitlePattern '^DSH' -TimeoutSec 30
    $result.restart_ok = $true
    Close-TopWindow -Window $mainWindow
    $null = Wait-DshCondition -TimeoutSec 20 -Message 'manager restart close' -Condition {
        $null -eq (Get-Process -Id $appProcess.Id -ErrorAction SilentlyContinue)
    }

    $result.ok = $true
    $exitCode = 0
}
catch {
    $result.error = $_.Exception.Message
    Write-Error $_
}
finally {
    if ($null -ne $appProcess) {
        $p = Get-Process -Id $appProcess.Id -ErrorAction SilentlyContinue
        if ($p) { Stop-DshProcessTree -RootPid $appProcess.Id }
    }

    if (-not $KeepTestInstances) {
        foreach ($id in $createdIds) {
            try {
                $null = Invoke-CliJson -Arguments @('instance', 'remove', $id, '--json')
                Write-Host "Removed test instance: $id"
            }
            catch { Write-Warning "Could not remove test instance ${id}: $($_.Exception.Message)" }
        }
        if (-not $NoRestoreConfig -and (Test-Path -LiteralPath $configBackup)) {
            Copy-Item -LiteralPath $configBackup -Destination $configPath -Force
            Write-Host 'Restored config.json'
        }
    }
    $result.finished_at = (Get-Date).ToString('o')
    Write-AcceptanceReport -Result $result -OutputDir $OutputDir -Name 'acceptance-instances'
}
exit $exitCode
