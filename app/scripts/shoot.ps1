#Requires -Version 5.1
<#
.SYNOPSIS
  Deterministic Task 7 screenshot capture (plan 052, UI Task 7 pre-work B3;
  fixed-wait method from the Task 7 fix wave).

.DESCRIPTION
  Builds alidade-app and its seed_demo helper, seeds a fresh demo database
  (and a matching settings file, F1) into a scratch temp path, then for each
  of the five screens x two themes launches the app with
  `--screen`/`--theme`/`--window`/`--db`/`--settings`, waits a plain 20 s,
  captures its client area once with System.Drawing, saves it to
  app/docs/screens/<screen>-<theme>.png, and kills the process. Ten PNGs.

  A polling wait right after launch was observed to leave the window at
  0x0 (a first-launch wgpu pipeline init can take over 15 s, and calling
  `SetForegroundWindow` before the window has sized itself was observed to
  stall the resize permanently). The fixed 20 s wait avoids both: launch,
  `Start-Sleep 20`, one `SetForegroundWindow`, one `GetClientRect`, one
  `CopyFromScreen` — no polling loop at all.

  Fails loudly (throws, non-zero exit) when a window never appears or its
  client area reads 0x0 after the wait, rather than writing a blank or
  missing screenshot.

  A second pair of screenshots — `settings-notifications-<theme>.png` — is
  captured with `--scroll settings-notifications` (F4): the Notifications
  block sits below the fold at 1280x800, so this proves it is reachable
  without guessing whether a tighter layout alone would fit it on screen.
  These two are in addition to the ten named above.
#>

$ErrorActionPreference = 'Stop'

$appRoot = Split-Path -Parent $PSScriptRoot           # apps/alidade/app
$repoRoot = Split-Path -Parent $appRoot               # apps/alidade
$screensDir = Join-Path $appRoot 'docs\screens'
New-Item -ItemType Directory -Force -Path $screensDir | Out-Null

# --- Build the app and the demo-data seeder (B3: "cargo build -p
# alidade-app --jobs 4 first") --------------------------------------------
Write-Host "building alidade-app and seed_demo ..."
& cargo build -p alidade-app --jobs 4 --manifest-path (Join-Path $repoRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) {
    throw "cargo build -p alidade-app failed with exit code $LASTEXITCODE"
}

$exe = Join-Path $repoRoot 'target\debug\alidade-app.exe'
$seedExe = Join-Path $repoRoot 'target\debug\seed_demo.exe'
if (-not (Test-Path -LiteralPath $exe)) {
    throw "built exe not found at $exe"
}
if (-not (Test-Path -LiteralPath $seedExe)) {
    throw "seed_demo exe not found at $seedExe"
}

# --- Seed the demo database and its matching settings file (B3: "literal
# path built at runtime and printed"; F1: settings live next to the DB so
# `--db` and `--settings` always name a paired set) -----------------------
$dbPath = Join-Path $env:TEMP 'alidade-shoot\demo.db'
$settingsPath = Join-Path $env:TEMP 'alidade-shoot\settings.toml'
Write-Host "demo DB path: $dbPath"
Write-Host "demo settings path: $settingsPath"
& $seedExe $dbPath
if ($LASTEXITCODE -ne 0) {
    throw "seed_demo failed with exit code $LASTEXITCODE"
}
if (-not (Test-Path -LiteralPath $settingsPath)) {
    throw "seed_demo did not write the expected settings file at $settingsPath"
}

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class AlidadeShootWin32 {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X; public int Y; }

    [DllImport("user32.dll")]
    public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool ClientToScreen(IntPtr hWnd, ref POINT lpPoint);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@

# How long to let the window boot and paint before the single capture
# (fixed-wait method, Task 7 fix wave: no polling right after launch).
$fixedWaitSeconds = 20

# Launches $exe with $ProcessArgs, waits the fixed 20 s, captures the client
# area exactly once, saves it to $OutPath, and kills the process. Throws
# loudly instead of writing a blank/missing screenshot.
function Invoke-AlidadeCapture {
    param(
        [string[]] $ProcessArgs,
        [string] $OutPath,
        [string] $Label
    )

    Write-Host "capturing $Label ..."
    $proc = Start-Process -FilePath $exe -ArgumentList $ProcessArgs -PassThru

    Start-Sleep -Seconds $fixedWaitSeconds

    $proc.Refresh()
    if ($proc.HasExited) {
        throw "alidade-app exited before capture for $Label (exit code $($proc.ExitCode))"
    }
    $hwnd = $proc.MainWindowHandle
    if ($hwnd -eq [IntPtr]::Zero) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        throw "no window handle for $Label after a $fixedWaitSeconds s wait"
    }

    [AlidadeShootWin32]::SetForegroundWindow($hwnd) | Out-Null

    $clientRect = New-Object AlidadeShootWin32+RECT
    [AlidadeShootWin32]::GetClientRect($hwnd, [ref]$clientRect) | Out-Null
    $width = $clientRect.Right - $clientRect.Left
    $height = $clientRect.Bottom - $clientRect.Top
    if ($width -le 0 -or $height -le 0) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        throw "window client area was empty ($width x $height) for $Label"
    }

    $origin = New-Object AlidadeShootWin32+POINT
    $origin.X = 0
    $origin.Y = 0
    [AlidadeShootWin32]::ClientToScreen($hwnd, [ref]$origin) | Out-Null

    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen(
            $origin.X, $origin.Y, 0, 0,
            (New-Object System.Drawing.Size $width, $height)
        )
        $bitmap.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }

    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue

    $size = (Get-Item -LiteralPath $OutPath).Length
    Write-Host "$OutPath : $size bytes"
    return "$OutPath : $size bytes"
}

$screens = @('home', 'continuous', 'ping', 'history', 'settings')
$themes = @('light', 'dark')

$results = New-Object System.Collections.Generic.List[string]

foreach ($screen in $screens) {
    foreach ($theme in $themes) {
        $processArgs = @(
            '--screen', $screen, '--theme', $theme, '--window', '1280x800',
            '--db', $dbPath, '--settings', $settingsPath
        )
        $outPath = Join-Path $screensDir "$screen-$theme.png"
        $results.Add((Invoke-AlidadeCapture -ProcessArgs $processArgs -OutPath $outPath -Label "$screen / $theme"))
    }
}

# F4: an extra pair, scrolled to the bottom of Settings, proving the
# Notifications block is reachable even though it sits below the fold at
# 1280x800. In addition to the ten above, not a replacement for either.
foreach ($theme in $themes) {
    $processArgs = @(
        '--screen', 'settings', '--theme', $theme, '--window', '1280x800',
        '--db', $dbPath, '--settings', $settingsPath,
        '--scroll', 'settings-notifications'
    )
    $outPath = Join-Path $screensDir "settings-notifications-$theme.png"
    $results.Add((Invoke-AlidadeCapture -ProcessArgs $processArgs -OutPath $outPath -Label "settings-notifications / $theme"))
}

Write-Host ""
Write-Host "done: $($results.Count) screenshots written to $screensDir"
foreach ($line in $results) { Write-Host $line }
