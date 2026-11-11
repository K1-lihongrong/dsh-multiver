# Shared helpers for dsh-multiver native acceptance scripts.
# Windows PowerShell 5.1 compatible.

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing

if (-not ('DshAcceptanceWin' -as [type])) {
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public class DshAcceptanceWin {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
}

$script:DshAcceptanceMouseDown = 0x0002
$script:DshAcceptanceMouseUp = 0x0004

function Get-DshProcesses {
    return @(Get-CimInstance Win32_Process -Filter "Name='dsh-multiver.exe'" -ErrorAction SilentlyContinue)
}

function Stop-DshProcessTree {
    param([int]$RootPid)

    $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $ids = [System.Collections.Generic.List[int]]::new()
    $ids.Add($RootPid)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($proc in $all) {
            if ($ids.Contains([int]$proc.ParentProcessId) -and -not $ids.Contains([int]$proc.ProcessId)) {
                $ids.Add([int]$proc.ProcessId)
                $changed = $true
            }
        }
    }
    foreach ($id in ($ids | Sort-Object -Descending)) {
        Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    }
}

function Get-DshTopWindows {
    param([int]$ProcessId)

    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $ProcessId)
    $windows = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
    $rows = @()
    foreach ($win in $windows) {
        try {
            $rect = $win.Current.BoundingRectangle
            if ([double]::IsInfinity($rect.Width) -or $rect.Width -lt 100) { continue }
            $rows += $win
        }
        catch {}
    }
    return $rows
}

function Wait-DshWindow {
    param(
        [int]$ProcessId,
        [string]$TitlePattern = '.',
        [int]$TimeoutSec = 30
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    do {
        foreach ($win in (Get-DshTopWindows -ProcessId $ProcessId)) {
            try {
                if ($win.Current.Name -match $TitlePattern) { return $win }
            }
            catch {}
        }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)

    throw "Window not found for pid $ProcessId, title pattern: $TitlePattern"
}

function Get-DshElements {
    param([System.Windows.Automation.AutomationElement]$Window)
    return $Window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
}

function Find-DshElement {
    param(
        [System.Windows.Automation.AutomationElement]$Window,
        [string]$ControlType = '',
        [string]$Name = '',
        [string]$NameRegex = '',
        [int]$Index = 0,
        [switch]$EnabledOnly
    )

    $wantedType = $ControlType
    if ($wantedType -and -not $wantedType.StartsWith('ControlType.')) {
        $wantedType = 'ControlType.' + $wantedType
    }

    $found = @()
    foreach ($el in (Get-DshElements -Window $Window)) {
        try {
            if ($wantedType -and $el.Current.ControlType.ProgrammaticName -ne $wantedType) { continue }
            if ($Name -and $el.Current.Name -ne $Name) { continue }
            if ($NameRegex -and $el.Current.Name -notmatch $NameRegex) { continue }
            if ($EnabledOnly -and -not $el.Current.IsEnabled) { continue }
            $found += $el
        }
        catch {}
    }

    if ($found.Count -le $Index) { return $null }
    return $found[$Index]
}

function Wait-DshElement {
    param(
        [System.Windows.Automation.AutomationElement]$Window,
        [string]$ControlType = '',
        [string]$Name = '',
        [string]$NameRegex = '',
        [int]$TimeoutSec = 15
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    do {
        $el = Find-DshElement -Window $Window -ControlType $ControlType -Name $Name -NameRegex $NameRegex
        if ($null -ne $el) { return $el }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)

    $desc = if ($Name) { $Name } elseif ($NameRegex) { $NameRegex } else { $ControlType }
    throw "Element not found: $desc"
}
function Invoke-DshElement {
    param(
        [System.Windows.Automation.AutomationElement]$Element,
        [switch]$Collapse
    )

    if ($null -eq $Element) { throw 'Cannot invoke a null UI element' }

    $pattern = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        $pattern.Invoke()
        Start-Sleep -Milliseconds 300
        return 'invoke'
    }

    $pattern = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
        $pattern.Toggle()
        Start-Sleep -Milliseconds 300
        return 'toggle'
    }

    $pattern = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
        if ($Collapse -or $pattern.Current.ExpandCollapseState -eq [System.Windows.Automation.ExpandCollapseState]::Expanded) {
            if ($pattern.Current.ExpandCollapseState -eq [System.Windows.Automation.ExpandCollapseState]::Expanded) {
                $pattern.Collapse()
            }
        }
        else {
            $pattern.Expand()
        }
        Start-Sleep -Milliseconds 300
        return 'expand-collapse'
    }

    $pattern = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
        $pattern.Select()
        Start-Sleep -Milliseconds 300
        return 'select'
    }

    $rect = $Element.Current.BoundingRectangle
    if ([double]::IsInfinity($rect.Width) -or [double]::IsInfinity($rect.Height) -or $rect.Width -le 0 -or $rect.Height -le 0) {
        throw "Element has no clickable rectangle: $($Element.Current.Name)"
    }
    $hwnd = [IntPtr]$Element.Current.NativeWindowHandle
    if ($hwnd -ne [IntPtr]::Zero) {
        [DshAcceptanceWin]::ShowWindow($hwnd, 9) | Out-Null
        [DshAcceptanceWin]::SetForegroundWindow($hwnd) | Out-Null
    }
    $x = [int]($rect.X + $rect.Width / 2)
    $y = [int]($rect.Y + $rect.Height / 2)
    [DshAcceptanceWin]::SetCursorPos($x, $y) | Out-Null
    [DshAcceptanceWin]::mouse_event($script:DshAcceptanceMouseDown, 0, 0, 0, [UIntPtr]::Zero)
    [DshAcceptanceWin]::mouse_event($script:DshAcceptanceMouseUp, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 400
    return 'mouse'
}

function Wait-DshCondition {
    param(
        [scriptblock]$Condition,
        [int]$TimeoutSec = 15,
        [string]$Message = 'condition'
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    do {
        if (& $Condition) { return $true }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)

    throw "Timeout waiting for $Message"
}

function Save-DshScreenshot {
    param(
        [System.Windows.Automation.AutomationElement]$Window,
        [string]$Path
    )

    $hwnd = [IntPtr]$Window.Current.NativeWindowHandle
    if ($hwnd -eq [IntPtr]::Zero) { throw 'Window has no native handle' }
    [DshAcceptanceWin]::ShowWindow($hwnd, 9) | Out-Null
    [DshAcceptanceWin]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 500

    $rect = New-Object DshAcceptanceWin+RECT
    [DshAcceptanceWin]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) { throw 'Invalid window rectangle' }

    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
    return $Path
}

function Get-FileSha256 {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}

function Read-AppendedText {
    param([string]$Path, [long]$Offset)

    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        if ($Offset -gt $stream.Length) { $Offset = 0 }
        $stream.Seek($Offset, [System.IO.SeekOrigin]::Begin) | Out-Null
        $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8, $true)
        try { return $reader.ReadToEnd() }
        finally { $reader.Dispose() }
    }
    finally { $stream.Dispose() }
}

function Write-AcceptanceReport {
    param(
        [object]$Result,
        [string]$OutputDir,
        [string]$Name
    )

    if (-not (Test-Path -LiteralPath $OutputDir)) {
        $null = New-Item -ItemType Directory -Path $OutputDir -Force
    }
    $jsonPath = Join-Path $OutputDir ($Name + '.json')
    $textPath = Join-Path $OutputDir ($Name + '.txt')
    $Result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $jsonPath -Encoding utf8
    ($Result | Format-List | Out-String) | Set-Content -LiteralPath $textPath -Encoding utf8
    Write-Host "REPORT JSON: $jsonPath"
    Write-Host "REPORT TEXT: $textPath"
}
function Read-TextShared {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8, $true)
        try { return $reader.ReadToEnd() }
        finally { $reader.Dispose() }
    }
    finally { $stream.Dispose() }
}