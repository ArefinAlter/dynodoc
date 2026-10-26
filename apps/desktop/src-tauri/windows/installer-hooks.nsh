; Hooks of the Dynodoc installer (Tauri's NSIS template calls these macros).
;
; The app starts a PostgreSQL database for the workspace on this computer and stops it when it
; closes. If the app was ended some other way the database may still be running from the install
; folder (on purpose: the next start adopts it), which would stop an update from replacing its files
; and an uninstall from removing them. Before either, ask that database to stop (a clean stop), then
; end whatever else is still running from the install folder (only programs whose file is under it).
;
; The installer is a 32-bit program: its System32 is redirected to the 32-bit PowerShell, which
; cannot read the path of a 64-bit process. `Sysnative` is the way to the 64-bit one.

!macro DYNODOC_STOP_WORKSPACE
  ${If} ${FileExists} "$INSTDIR\pg\bin\pg_ctl.exe"
  ${AndIf} ${FileExists} "$LOCALAPPDATA\online.dynodoc.client\workspace\pgdata\postmaster.pid"
    nsExec::Exec `"$INSTDIR\pg\bin\pg_ctl.exe" stop -D "$LOCALAPPDATA\online.dynodoc.client\workspace\pgdata" -m fast -t 25 -s`
    Pop $0
  ${EndIf}
  StrCpy $2 "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${IfNot} ${FileExists} "$2"
    StrCpy $2 "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
  nsExec::Exec `"$2" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-Process | Where-Object { $$_.Path -and $$_.Path -like '$INSTDIR\*' -and $$_.Id -ne $$PID } | Stop-Process -Force"`
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro DYNODOC_STOP_WORKSPACE
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro DYNODOC_STOP_WORKSPACE
!macroend
