#requires -Version 5.1
<#
Safe replacement for the destructive compat-matrix.ps1.

Important:
- This script is a compatibility smoke test, not a full product verdict.
- It checks plugin add/remove/add, dsh web startup, authenticated state,
  unauthenticated 401, and authenticated non-POST 405.
- Review version-specific failures before updating the official matrix.

Default is dry-run. Pass -Execute only after reviewing the paths printed at startup.
#>
[CmdletBinding()]
param(
    [string]$VersionRoot = 'E:\.LLM-AGENTS\deepseek-harness\versions',
    [string]$PluginTgz = 'E:\.Vibe_Coding\deepseek-Desktop\dsh-multiver\dsh-plugin\dsh-multiver-bridge-0.2.1.tgz',
    [int]$StartupTimeoutSec = 30,
    [string[]]$OnlyVersions = @(),
    [string]$RealConfig = 'E:\.LLM-AGENTS\deepseek-harness\config.json',
    [switch]$Execute
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$defaultVersions = @(
    '0.1.5-rc.3','0.1.7-rc.2','0.2.0-rc.1','0.2.0-rc.2',
    '0.2.1-rc.2','0.2.1-rc.3','0.2.1-rc.4','0.2.1-rc.5',
    '0.2.2-alpha.2','0.2.2-alpha.3','0.2.2-rc.1'
)
$versions = if ($OnlyVersions.Count -gt 0) { $OnlyVersions } else { $defaultVersions }

if (-not $Execute) {
    Write-Host 'DRY RUN: no plugin install, no dsh web startup, no deletion.'
    Write-Host "Version root: $VersionRoot"
    Write-Host "Plugin tgz:   $PluginTgz"
    Write-Host "Real config:  $RealConfig"
    Write-Host ('Versions:     ' + ($versions -join ', '))
    Write-Host 'Rerun with -Execute to perform the isolated compatibility matrix.'
    exit 0
}

function Assert-UnderRoot {
    param([string]$Path, [string]$Root)
    $full = [IO.Path]::GetFullPath($Path)
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to operate outside test root. path=$full root=$Root"
    }
}

function Stop-Tree {
    param([int]$RootPid)
    $all = Get-CimInstance Win32_Process
    $ids = [System.Collections.Generic.List[int]]::new()
    $ids.Add($RootPid)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($p in $all) {
            if ($ids.Contains([int]$p.ParentProcessId) -and -not $ids.Contains([int]$p.ProcessId)) {
                $ids.Add([int]$p.ProcessId)
                $changed = $true
            }
        }
    }
    foreach ($id in ($ids | Sort-Object -Descending)) {
        Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    }
}

# Run a native command without letting stderr become a terminating error under
# PowerShell 5.1. Return output and exit code separately.
function Invoke-NativeCapture {
    param([string]$FilePath, [string[]]$Arguments)

    $oldEap = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & $FilePath @Arguments 2>&1 | Out-String
        $exitCode = $LASTEXITCODE
        return [pscustomobject]@{
            ExitCode = $exitCode
            Output = $output
        }
    }
    finally {
        $ErrorActionPreference = $oldEap
    }
}

# Use curl.exe for cookie bootstrap and HTTP status checks. This avoids
# PowerShell 5.1 Invoke-WebRequest quirks and is available on Windows 10+.
function Test-BridgeProtocol {
    param([string]$LaunchUrl, [string]$WorkDir)

    $check = [ordered]@{
        state_200 = $false
        state_ok = $false
        auth_401 = $false
        method_405 = $false
        error = ''
    }

    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if ($null -eq $curl) {
        $check.error = 'curl.exe not found'
        return [pscustomobject]$check
    }

    $cookieJar = Join-Path $WorkDir ('cookies-' + [guid]::NewGuid().ToString('N') + '.txt')
    $bodyFile = Join-Path $WorkDir ('body-' + [guid]::NewGuid().ToString('N') + '.json')

    try {
        $uri = [Uri]$LaunchUrl
        $stateUrl = '{0}://{1}/multiver-bridge/state' -f $uri.Scheme, $uri.Authority

        $bootstrap = Invoke-NativeCapture -FilePath $curl.Source -Arguments @(
            '-sS', '-L', '-c', $cookieJar, '-o', $bodyFile, '-w', '%{http_code}', $LaunchUrl
        )
        $bootstrapStatus = ([string]$bootstrap.Output).Trim()
        if ($bootstrap.ExitCode -ne 0 -or $bootstrapStatus -ne '200') {
            $check.error = 'token bootstrap failed: exit={0} http={1} {2}' -f $bootstrap.ExitCode, $bootstrapStatus, (([string]$bootstrap.Output).Trim())
            return [pscustomobject]$check
        }

        $stateRun = Invoke-NativeCapture -FilePath $curl.Source -Arguments @(
            '-sS', '-b', $cookieJar, '-X', 'POST',
            '-H', 'Content-Type: application/json', '-d', '{}',
            '-o', $bodyFile, '-w', '%{http_code}', $stateUrl
        )
        $stateStatus = ([string]$stateRun.Output).Trim()
        $check.state_200 = ($stateRun.ExitCode -eq 0 -and $stateStatus -eq '200')
        if ($check.state_200 -and (Test-Path -LiteralPath $bodyFile)) {
            try {
                $stateJson = [IO.File]::ReadAllText($bodyFile, [Text.Encoding]::UTF8)
                $stateBody = $stateJson | ConvertFrom-Json
                $check.state_ok = ($stateBody.ok -eq $true)
            }
            catch {
                $check.error = 'state response is not valid JSON'
            }
        }
        elseif (-not $check.state_200) {
            $check.error = 'state request failed: exit={0} http={1}' -f $stateRun.ExitCode, $stateStatus
        }

        $unauthRun = Invoke-NativeCapture -FilePath $curl.Source -Arguments @(
            '-sS', '-X', 'POST',
            '-H', 'Content-Type: application/json', '-d', '{}',
            '-o', $bodyFile, '-w', '%{http_code}', $stateUrl
        )
        $unauthStatus = ([string]$unauthRun.Output).Trim()
        $check.auth_401 = ($unauthRun.ExitCode -eq 0 -and $unauthStatus -eq '401')
        if (-not $check.auth_401 -and -not $check.error) {
            $check.error = 'unauthenticated probe expected 401, got exit={0} http={1}' -f $unauthRun.ExitCode, $unauthStatus
        }

        $methodRun = Invoke-NativeCapture -FilePath $curl.Source -Arguments @(
            '-sS', '-b', $cookieJar, '-X', 'GET',
            '-o', $bodyFile, '-w', '%{http_code}', $stateUrl
        )
        $methodStatus = ([string]$methodRun.Output).Trim()
        $check.method_405 = ($methodRun.ExitCode -eq 0 -and $methodStatus -eq '405')
        if (-not $check.method_405 -and -not $check.error) {
            $check.error = 'authenticated GET expected 405, got exit={0} http={1}' -f $methodRun.ExitCode, $methodStatus
        }
    }
    catch {
        $check.error = ("" + $_).Trim()
    }
    finally {
        Remove-Item -LiteralPath $cookieJar, $bodyFile -Force -ErrorAction SilentlyContinue
    }

    return [pscustomobject]$check
}
$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
$testRoot = Join-Path $env:TEMP ("dsh-multiver-compat-" + $runId + '-' + $PID)
Assert-UnderRoot $testRoot $env:TEMP
$null = New-Item -ItemType Directory -Path $testRoot -Force

Write-Host "TEST ROOT: $testRoot"
Write-Host "VERSION ROOT: $VersionRoot"
Write-Host "PLUGIN: $PluginTgz"
if (-not (Test-Path -LiteralPath $PluginTgz)) { throw "Plugin tgz not found: $PluginTgz" }

$oldDshHome = $env:DSH_HOME
$configHashBefore = $null
if (Test-Path -LiteralPath $RealConfig) {
    $configHashBefore = (Get-FileHash -LiteralPath $RealConfig -Algorithm SHA256).Hash
    Write-Host "REAL CONFIG SNAPSHOT: $RealConfig $configHashBefore"
}
else {
    Write-Warning "Real config not found, hash check disabled: $RealConfig"
}
$results = @()

try {
    foreach ($ver in $versions) {
        $testHome = Join-Path $testRoot $ver
        Assert-UnderRoot $testHome $testRoot
        Remove-Item -LiteralPath $testHome -Recurse -Force -ErrorAction SilentlyContinue
        $null = New-Item -ItemType Directory -Path $testHome -Force

        $env:DSH_HOME = $testHome
        if ([IO.Path]::GetFullPath($env:DSH_HOME) -ne [IO.Path]::GetFullPath($testHome)) {
            throw "DSH_HOME isolation check failed: $env:DSH_HOME"
        }

        $row = [ordered]@{
            version = $ver
            test_home = $testHome
            add1 = $null
            add1_present = $false
            remove = $null
            removed = $false
            add2 = $null
            add2_present = $false
            start_url = $false
            state_200 = $false
            state_ok = $false
            auth_401 = $false
            method_405 = $false
            error = ''
        }

        $dsh = Join-Path $VersionRoot (Join-Path $ver 'node_modules\.bin\dsh.cmd')
        $pkg = Join-Path $testHome 'profiles\web\package.json'
        Write-Host "=== $ver ==="
        Write-Host "DSH_HOME=$env:DSH_HOME"

        if (-not (Test-Path -LiteralPath $dsh)) {
            $row.error = "dsh entry not found: $dsh"
            $results += [pscustomobject]$row
            continue
        }

        try {
            $add1Run = Invoke-NativeCapture -FilePath $dsh -Arguments @('plugin', '--profile', 'web', 'add', $PluginTgz)
            $row.add1 = $add1Run.ExitCode
            if (Test-Path -LiteralPath $pkg) {
                $row.add1_present = [bool]((Get-Content -LiteralPath $pkg -Raw) -match 'dsh-multiver-bridge')
            }

            $removeRun = Invoke-NativeCapture -FilePath $dsh -Arguments @('plugin', '--profile', 'web', 'remove', 'dsh-multiver-bridge')
            $row.remove = $removeRun.ExitCode
            if (Test-Path -LiteralPath $pkg) {
                $row.removed = -not [bool]((Get-Content -LiteralPath $pkg -Raw) -match 'dsh-multiver-bridge')
            }

            $add2Run = Invoke-NativeCapture -FilePath $dsh -Arguments @('plugin', '--profile', 'web', 'add', $PluginTgz)
            $row.add2 = $add2Run.ExitCode
            if (Test-Path -LiteralPath $pkg) {
                $row.add2_present = [bool]((Get-Content -LiteralPath $pkg -Raw) -match 'dsh-multiver-bridge')
            }

            $log = Join-Path $testRoot ($ver + '.log')
            $err = Join-Path $testRoot ($ver + '.log.err')
            $proc = Start-Process -FilePath 'cmd.exe' `
                -ArgumentList '/c', "`"$dsh`" web --port 0 --no-open" `
                -RedirectStandardOutput $log -RedirectStandardError $err `
                -PassThru -WindowStyle Hidden

            try {
                $launchUrl = $null
                for ($i = 0; $i -lt ($StartupTimeoutSec * 2); $i++) {
                    Start-Sleep -Milliseconds 500
                    if (Test-Path -LiteralPath $log) {
                        $logText = Get-Content -LiteralPath $log -Raw -ErrorAction SilentlyContinue
                        if ($null -eq $logText) { $logText = '' }
                        $urlMatch = [regex]::Match($logText, 'http://127\.0\.0\.1:\d+/\?token=[^\s]+')
                        if ($urlMatch.Success) {
                            $row.start_url = $true
                            $launchUrl = $urlMatch.Value
                            break
                        }
                    }
                    if ($proc.HasExited) { break }
                }

                if ($row.start_url) {
                    $protocol = Test-BridgeProtocol -LaunchUrl $launchUrl -WorkDir $testRoot
                    $row.state_200 = $protocol.state_200
                    $row.state_ok = $protocol.state_ok
                    $row.auth_401 = $protocol.auth_401
                    $row.method_405 = $protocol.method_405
                    if ($protocol.error) { $row.error = $protocol.error }
                }
                else {
                    $tail = ''
                    if (Test-Path -LiteralPath $log) { $tail += (Get-Content -LiteralPath $log -Raw -ErrorAction SilentlyContinue) }
                    if (Test-Path -LiteralPath $err) { $tail += ' ' + (Get-Content -LiteralPath $err -Raw -ErrorAction SilentlyContinue) }
                    $tail = ($tail -replace '\s+', ' ').Trim()
                    if ($tail.Length -gt 400) { $tail = $tail.Substring(0, 400) }
                    $row.error = $tail
                }
            }
            finally {
                if ($proc) {
                    if (-not $proc.HasExited) { Stop-Tree $proc.Id }
                    try { $null = $proc.WaitForExit(5000) } catch {}
                    $proc.Dispose()
                }
            }
        }
        catch {
            $row.error = ("" + $_).Trim()
        }

        $results += [pscustomobject]$row
    }
}
finally {
    if ($null -eq $oldDshHome) {
        Remove-Item Env:DSH_HOME -ErrorAction SilentlyContinue
    }
    else {
        $env:DSH_HOME = $oldDshHome
    }

    if (Test-Path -LiteralPath $testRoot) {
        Assert-UnderRoot $testRoot $env:TEMP
        $cleanupOk = $false
        for ($attempt = 1; $attempt -le 10; $attempt++) {
            try {
                Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction Stop
                $cleanupOk = $true
                break
            }
            catch {
                if ($attempt -eq 10) {
                    Write-Warning ("Could not remove test root after retries: {0}" -f $testRoot)
                }
                else {
                    Start-Sleep -Milliseconds 300
                }
            }
        }
        if (-not $cleanupOk) {
            Write-Warning ("Test root left for inspection: {0}" -f $testRoot)
        }
    }
}

$configHashAfter = $null
if (Test-Path -LiteralPath $RealConfig) {
    $configHashAfter = (Get-FileHash -LiteralPath $RealConfig -Algorithm SHA256).Hash
}
$configUnchanged = ($configHashBefore -eq $configHashAfter)
if (-not $configUnchanged) {
    Write-Warning ("Real config changed during matrix run: {0} -> {1}" -f $configHashBefore, $configHashAfter)
}
else {
    Write-Host "REAL CONFIG UNCHANGED: $configHashAfter"
}

$results = @($results | ForEach-Object {
    $null = $_ | Add-Member -NotePropertyName real_config_unchanged -NotePropertyValue $configUnchanged -PassThru
    $ok = (
        $_.add1 -eq 0 -and $_.add1_present -and
        $_.remove -eq 0 -and $_.removed -and
        $_.add2 -eq 0 -and $_.add2_present -and
        $_.start_url -and $_.state_200 -and $_.state_ok -and
        $_.auth_401 -and $_.method_405 -and
        $_.real_config_unchanged -and
        [string]::IsNullOrWhiteSpace([string]$_.error)
    )
    $_ | Add-Member -NotePropertyName all_ok -NotePropertyValue $ok -PassThru
})

$passed = @($results | Where-Object { $_.all_ok }).Count
Write-Host ("Compatibility smoke checks: {0}/{1} passed" -f $passed, $results.Count)

$outputDir = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\.tmp\compat-matrix'))
if (-not (Test-Path -LiteralPath $outputDir)) { $null = New-Item -ItemType Directory -Path $outputDir -Force }
$jsonPath = Join-Path $outputDir 'compat-matrix-results.json'
$textPath = Join-Path $outputDir 'compat-matrix-results.txt'
$results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $jsonPath -Encoding utf8
$results | Format-Table -AutoSize | Out-String | Set-Content -LiteralPath $textPath -Encoding utf8
$results | Format-Table -AutoSize
Write-Host "REPORT JSON: $jsonPath"
Write-Host "REPORT TEXT: $textPath"
if ($passed -ne $results.Count) { exit 1 }
exit 0