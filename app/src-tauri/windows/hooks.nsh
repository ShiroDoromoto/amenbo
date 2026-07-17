; hooks.nsh — Tauri NSIS installer hooks for amenbo.
;
; The unified installer ships the amenbo CLI as a Tauri sidecar (externalBin);
; NSIS installs it into $INSTDIR as `amenbo.exe`, next to the GUI
; `amenbo-app.exe`. These hooks put $INSTDIR on the *user* PATH so `amenbo`
; works from any terminal — one installer lands GUI + CLI on PATH.
;
; Per-user PATH (HKCU\Environment) matches Tauri's default `currentUser`
; install mode. WriteRegExpandStr keeps PATH a REG_EXPAND_SZ; the
; WM_SETTINGCHANGE broadcast lets already-open shells and Explorer pick up the
; change without a reboot (new terminals only — an open shell keeps its own copy).

!include "LogicLib.nsh"
!include "WinMessages.nsh"
!include "StrFunc.nsh"

; StrFunc requires each used function be declared once, at global scope, before use.
${StrStr}    ; installer:   substring search (idempotent-append guard)
${UnStrRep}  ; uninstaller: substring replace (PATH segment removal)

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Registering amenbo CLI on PATH…"
  ReadRegStr $0 HKCU "Environment" "Path"
  ; Append $INSTDIR only if absent, so re-installs and in-place updates don't
  ; grow PATH with duplicates.
  ${StrStr} $1 "$0" "$INSTDIR"
  ${If} $1 == ""
    ${If} $0 == ""
      StrCpy $0 "$INSTDIR"
    ${Else}
      StrCpy $0 "$0;$INSTDIR"
    ${EndIf}
    WriteRegExpandStr HKCU "Environment" "Path" "$0"
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DetailPrint "Removing amenbo CLI from PATH…"
  ReadRegStr $0 HKCU "Environment" "Path"
  ; Strip our segment. Handle the sole-entry case exactly (avoids a bare
  ; mid-string replace that could clip a longer path sharing our prefix);
  ; otherwise drop the "…;$INSTDIR" / "$INSTDIR;…" forms.
  ${If} $0 == "$INSTDIR"
    StrCpy $1 ""
  ${Else}
    ${UnStrRep} $1 "$0" ";$INSTDIR" ""
    ${UnStrRep} $1 "$1" "$INSTDIR;" ""
  ${EndIf}
  WriteRegExpandStr HKCU "Environment" "Path" "$1"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend
