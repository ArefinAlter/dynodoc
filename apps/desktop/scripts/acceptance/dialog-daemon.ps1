# Answers the standard Windows file dialogs of an installed Dynodoc (the release build has no test seam
# for them), as a person would: by what they are called. It runs until the file named by -StopFile exists.
#   powershell -File dialog-daemon.ps1 -InstallDir D:\x\Dynodoc -Answers answers.json -StopFile stop.txt
# answers.json maps a dialog's title to what to type: {"Add files to this project": {"folder": "D:\\samples",
# "names": "\"a.docx\" \"b.xlsx\""}, "Save a backup of this workspace": {"names": "D:\\x\\backup.zip"}}.
# Each dialog is answered once. The dialogs are plain windows: their controls get window messages.
param(
  [Parameter(Mandatory = $true)][string]$InstallDir,
  [Parameter(Mandatory = $true)][string]$Answers,
  [Parameter(Mandatory = $true)][string]$StopFile,
  [string]$Log = ''
)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Win {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, int m, IntPtr w, string l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, int m, IntPtr w, IntPtr l);
}
'@
$AE = [System.Windows.Automation.AutomationElement]
$map = Get-Content -Raw $Answers | ConvertFrom-Json
$done = @{}
function Note($text) { if ($Log -ne '') { Add-Content -Path $Log -Value ("{0} {1}" -f (Get-Date -Format HH:mm:ss), $text) } }
function Find-Control($dialog, $id, $class) {
  $all = $dialog.FindAll([System.Windows.Automation.TreeScope]::Descendants, (New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, $id)))
  foreach ($e in $all) { if ($e.Current.ClassName -eq $class) { return $e } }
  return $null
}
function Answer($dialog, $text) {
  $edit = Find-Control $dialog '1148' 'Edit'
  if (-not $edit) { $edit = Find-Control $dialog '1152' 'Edit' }
  $button = Find-Control $dialog '1' 'Button'
  if (-not $edit -or -not $button) { return $false }
  [void][Win]::SendMessage([IntPtr]$edit.Current.NativeWindowHandle, 0x000C, [IntPtr]::Zero, $text)
  [void][Win]::SendMessage([IntPtr]$button.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
  return $true
}
while (-not (Test-Path $StopFile)) {
  $ids = @(Get-Process | Where-Object { $_.Path -and $_.Path -like "$InstallDir*" } | ForEach-Object { $_.Id })
  foreach ($procId in $ids) {
    $top = $AE::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, (New-Object System.Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $procId)))
    foreach ($w in $top) {
      if ($w.Current.ClassName -ne '#32770') { continue }
      $key = "$($w.Current.NativeWindowHandle)"
      if ($done.ContainsKey($key)) { continue }
      $title = $w.Current.Name
      $a = $map.$title
      if (-not $a) { Note "dialog without an answer: $title"; $done[$key] = $true; continue }
      Start-Sleep -Milliseconds 700
      if ($a.folder) { [void](Answer $w $a.folder); Start-Sleep -Milliseconds 900 }
      $ok = Answer $w $a.names
      Note ("answered '{0}': {1}" -f $title, $ok)
      $done[$key] = $true
    }
  }
  Start-Sleep -Milliseconds 400
}
