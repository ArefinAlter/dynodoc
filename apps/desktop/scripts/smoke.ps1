<#
.SYNOPSIS
  Smoke test for a DEBUG build of the Dynodoc desktop app on Windows.

.DESCRIPTION
  Uses a throw-away home folder (DYNODOC_DESKTOP_HOME, honoured by debug builds only), so a
  real profile is never touched. Three phases:
    1. First run: the app starts, creates its config and data folders, shows a window titled
       "Dynodoc", stays up, and exits with code 0 when asked to close.
    2. Configured server: with a saved server (a local listener on 127.0.0.1) the window
       loads that address on start-up.
    3. Deep link: a second launch with dynodoc://open/workspace is handed to the running
       copy (the second process exits), which loads /workspace on the same server; a refused
       link (dynodoc://open/admin) loads nothing.
  Needs no network and no installed WebView2 beyond the one Windows 11 ships with.

.PARAMETER Exe
  Path to the debug executable. Default: $env:CARGO_TARGET_DIR\debug\dynodoc-desktop.exe,
  or src-tauri\target\debug\dynodoc-desktop.exe.

.PARAMETER ScreenshotDir
  Optional. Saves a PNG of the window in each phase there.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\smoke.ps1
#>
param(
  [string]$Exe,
  [int]$StaySeconds = 6,
  [string]$ScreenshotDir
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

if (-not $Exe) {
  $targetDir = $env:CARGO_TARGET_DIR
  if (-not $targetDir) { $targetDir = Join-Path $scriptRoot '..\src-tauri\target' }
  $Exe = Join-Path $targetDir 'debug\dynodoc-desktop.exe'
}
if (-not (Test-Path -LiteralPath $Exe)) { throw "Executable not found: $Exe (run: npm run tauri -- build --debug --no-bundle)" }
$Exe = (Resolve-Path -LiteralPath $Exe).Path

if (Get-Process -Name 'dynodoc-desktop' -ErrorAction SilentlyContinue) {
  throw 'Dynodoc is already running. Close it first: the single-instance lock is shared.'
}

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class SmokeNative {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
}
'@
[void][SmokeNative]::SetProcessDPIAware()

$failures = New-Object System.Collections.Generic.List[string]
function Check([bool]$ok, [string]$what) {
  if ($ok) { Write-Host "PASS  $what" } else { Write-Host "FAIL  $what"; $failures.Add($what) }
}

function Save-Shot($process, [string]$name) {
  if (-not $ScreenshotDir) { return }
  New-Item -ItemType Directory -Force -Path $ScreenshotDir | Out-Null
  $process.Refresh()
  $rect = New-Object SmokeNative+RECT
  if (-not [SmokeNative]::GetWindowRect($process.MainWindowHandle, [ref]$rect)) { return }
  [void][SmokeNative]::SetForegroundWindow($process.MainWindowHandle)
  Start-Sleep -Milliseconds 400
  $width = $rect.Right - $rect.Left
  $height = $rect.Bottom - $rect.Top
  $bitmap = New-Object System.Drawing.Bitmap $width, $height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
  $path = Join-Path $ScreenshotDir "$name.png"
  $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $graphics.Dispose(); $bitmap.Dispose()
  Write-Host "      screenshot: $path"
}

function Start-App([string[]]$AppArgs) {
  if ($AppArgs) { $p = Start-Process -FilePath $Exe -ArgumentList $AppArgs -PassThru }
  else { $p = Start-Process -FilePath $Exe -PassThru }
  $null = $p.Handle   # keep the handle so ExitCode can be read after exit
  return $p
}

function Wait-Window($process, [int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    $process.Refresh()
    if ($process.HasExited) { return $false }
    if ($process.MainWindowHandle -ne [IntPtr]::Zero) { return $true }
    Start-Sleep -Milliseconds 200
  }
  return $false
}

function Close-Clean($process, [string]$phase) {
  $process.Refresh()
  if ($process.HasExited) { Check $false "$phase`: still running before the close request"; return }
  [void]$process.CloseMainWindow()
  $exited = $process.WaitForExit(20000)
  Check $exited "$phase`: exits when its window is closed"
  if ($exited) { Check ($process.ExitCode -eq 0) "$phase`: exit code is 0 (got $($process.ExitCode))" }
}

# Read the first request line a client sends to the listener, and answer it.
function Read-Request($listener, [int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    if ($listener.Pending()) {
      $client = $listener.AcceptTcpClient()
      try {
        $stream = $client.GetStream()
        $stream.ReadTimeout = 3000
        $buffer = New-Object byte[] 8192
        $read = $stream.Read($buffer, 0, $buffer.Length)
        $text = [System.Text.Encoding]::ASCII.GetString($buffer, 0, $read)
        $body = '<!doctype html><title>smoke server</title><h1>smoke server</h1><p>Hello from the smoke test.</p>'
        $reply = "HTTP/1.1 200 OK`r`nContent-Type: text/html; charset=utf-8`r`nContent-Length: $($body.Length)`r`nConnection: close`r`n`r`n$body"
        $bytes = [System.Text.Encoding]::ASCII.GetBytes($reply)
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush()
        return ($text -split "`r?`n")[0]
      } finally { $client.Close() }
    }
    Start-Sleep -Milliseconds 100
  }
  return $null
}

$home1 = Join-Path ([System.IO.Path]::GetTempPath()) ("dynodoc-smoke-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $home1 | Out-Null
$env:DYNODOC_DESKTOP_HOME = $home1
$running = @()
$listener = $null

try {
  Write-Host "exe:  $Exe"
  Write-Host "home: $home1"

  # ---- Phase 1: first run ------------------------------------------------------------
  $p = Start-App
  $running += $p
  $hasWindow = Wait-Window $p 30
  Check $hasWindow 'first run: a window appears'
  Start-Sleep -Seconds $StaySeconds
  $p.Refresh()
  Check (-not $p.HasExited) "first run: process is still up after $StaySeconds seconds"
  Check (Test-Path -LiteralPath (Join-Path $home1 'config')) 'first run: config folder was created'
  Check (Test-Path -LiteralPath (Join-Path $home1 'data')) 'first run: data folder was created'
  Check (-not (Test-Path -LiteralPath (Join-Path $home1 'config\config.json'))) 'first run: no server is saved yet'
  Check ($p.MainWindowTitle -eq 'Dynodoc') "first run: window title is 'Dynodoc' (got '$($p.MainWindowTitle)')"
  Save-Shot $p 'launcher'
  Close-Clean $p 'first run'

  # ---- Phases 2 and 3: a saved server --------------------------------------------------
  $listener = New-Object System.Net.Sockets.TcpListener ([System.Net.IPAddress]::Loopback), 0
  $listener.Start()
  $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
  $config = '{"version":1,"server":"http://127.0.0.1:' + $port + '"}'
  [System.IO.File]::WriteAllText((Join-Path $home1 'config\config.json'), $config)

  $p = Start-App
  $running += $p
  $line = Read-Request $listener 40
  Check ($line -eq 'GET / HTTP/1.1') "saved server: the window requests the server's front page (got '$line')"
  Start-Sleep -Seconds 2
  Save-Shot $p 'server'

  # Phase 3: a second launch carrying a link is handed to the running copy.
  $second = Start-App @('dynodoc://open/workspace')
  $exitedFast = $second.WaitForExit(20000)
  Check $exitedFast 'deep link: the second launch exits (single instance)'
  $line = Read-Request $listener 20
  Check ($line -eq 'GET /workspace HTTP/1.1') "deep link: the running window loads /workspace on the saved server (got '$line')"

  $refused = Start-App @('dynodoc://open/admin')
  [void]$refused.WaitForExit(20000)
  $line = Read-Request $listener 4
  Check ($null -eq $line) "deep link: a refused link loads nothing (got '$line')"

  $evil = Start-App @('dynodoc://open/projects/0b0f6a52-6a0e-4c1f-9d57-5b7a3a1e2c44?token=abc')
  [void]$evil.WaitForExit(20000)
  $line = Read-Request $listener 4
  Check ($null -eq $line) "deep link: a link carrying a token loads nothing (got '$line')"

  Close-Clean $p 'saved server'
}
finally {
  foreach ($proc in $running) { try { if (-not $proc.HasExited) { $proc.Kill() } } catch {} }
  if ($listener) { $listener.Stop() }
  Remove-Item Env:\DYNODOC_DESKTOP_HOME -ErrorAction SilentlyContinue
  if ($home1 -like (Join-Path ([System.IO.Path]::GetTempPath()) 'dynodoc-smoke-*')) {
    Remove-Item -LiteralPath $home1 -Recurse -Force -ErrorAction SilentlyContinue
  }
}

if ($failures.Count -gt 0) {
  Write-Host ''
  Write-Host "$($failures.Count) check(s) failed."
  exit 1
}
Write-Host ''
Write-Host 'Smoke test passed.'
