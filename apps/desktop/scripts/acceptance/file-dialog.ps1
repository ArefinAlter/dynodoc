# Answers a standard Windows "Open" dialog that the app opened (the release build has no test seam for its
# file dialogs). Waits for the dialog of the given process, writes the path or names into the file name box
# and presses Open, with the dialog's own window messages (the controls of this dialog are plain windows,
# UI Automation shows them without patterns). Nothing is installed.
#   powershell -File file-dialog.ps1 -ProcessId 1234 -Names '"a.docx" "b.xlsx"' -Folder 'D:\samples'
param(
  [Parameter(Mandatory = $true)][int]$ProcessId,
  [Parameter(Mandatory = $true)][string]$Names,
  [string]$Folder = '',
  [int]$TimeoutSeconds = 30
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
function Find-Control($dialog, $id, $class) {
  $all = $dialog.FindAll([System.Windows.Automation.TreeScope]::Descendants, (New-Object System.Windows.Automation.PropertyCondition($AE::AutomationIdProperty, $id)))
  foreach ($e in $all) { if ($e.Current.ClassName -eq $class) { return $e } }
  return $null
}
$root = $AE::RootElement
$dialog = $null
$until = (Get-Date).AddSeconds($TimeoutSeconds)
while ((Get-Date) -lt $until -and -not $dialog) {
  $top = $root.FindAll([System.Windows.Automation.TreeScope]::Children, (New-Object System.Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $ProcessId)))
  foreach ($w in $top) { if ($w.Current.ClassName -eq '#32770') { $dialog = $w; break } }
  if (-not $dialog) { Start-Sleep -Milliseconds 300 }
}
if (-not $dialog) { Write-Output 'no dialog'; exit 2 }
Write-Output ('dialog: ' + $dialog.Current.Name)
Start-Sleep -Milliseconds 500
function Answer($text) {
  $edit = Find-Control $dialog '1148' 'Edit'
  if (-not $edit) { $edit = Find-Control $dialog '1152' 'Edit' }
  $open = Find-Control $dialog '1' 'Button'
  if (-not $edit -or -not $open) { Write-Output 'controls not found'; exit 3 }
  [void][Win]::SendMessage([IntPtr]$edit.Current.NativeWindowHandle, 0x000C, [IntPtr]::Zero, $text)
  [void][Win]::SendMessage([IntPtr]$open.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
}
if ($Folder -ne '') { Answer $Folder; Start-Sleep -Milliseconds 900 }
Answer $Names
Write-Output 'answered'
