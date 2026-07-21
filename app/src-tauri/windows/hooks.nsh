; hooks.nsh — Tauri NSIS installer hooks for amenbo.
;
; The unified installer ships the amenbo CLI as a Tauri sidecar (externalBin);
; NSIS installs it into $INSTDIR as `amenbo.exe`, next to the GUI
; `amenbo-app.exe`. These hooks put $INSTDIR on the *user* PATH so `amenbo`
; works from any terminal — one installer lands GUI + CLI on PATH.
;
; Per-user PATH (HKCU\Environment) matches the `currentUser` install mode set
; in tauri.conf.json — INSTDIR under %LOCALAPPDATA%, no UAC, so the app can
; self-replace without elevation. WriteRegExpandStr keeps PATH a REG_EXPAND_SZ; the
; WM_SETTINGCHANGE broadcast lets already-open shells and Explorer pick up the
; change without a reboot (new terminals only — an open shell keeps its own copy).
;
; POSTINSTALL also retires a leftover system-wide (perMachine) install from a
; pre-per-user release — a one-time migration, the only elevation in the
; per-user lifetime (self-update never asks). See the block for how it stays
; idempotent and channel-safe.

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

  ; One-time migration: retire an old system-wide (perMachine) install left over
  ; from a pre-per-user release. Older builds installed under Program Files and
  ; put the app + CLI on the *machine* (HKLM) — with the move to currentUser they
  ; must go, or the old copy shadows the freshly installed per-user one (the
  ; version-skew orphan self-update exists to avoid). New is already staged above
  ; (new before old, so no gap).
  ;
  ; Detect by the perMachine uninstall registration. currentUser writes ${UNINSTKEY}
  ; under HKCU; a perMachine install writes the *same* subkey under HKLM. We pinned
  ; currentUser (tauri.conf.json), so an HKLM registration can only be the old
  ; system copy — and ${PRODUCTNAME} is channel-specific (a dev build never matches
  ; the prod key), so this is channel-safe.
  ReadRegStr $2 HKLM "${UNINSTKEY}" "UninstallString"
  ${If} $2 != ""
    ; UninstallString is written quoted (`"…\uninstall.exe"`); strip the quotes.
    StrCpy $3 $2 1
    ${If} $3 == '"'
      StrCpy $2 $2 "" 1
      StrCpy $2 $2 -1
    ${EndIf}
    ${If} ${FileExists} "$2"
      DetailPrint "Retiring the old system-wide amenbo (one-time, needs elevation)…"
      ; Run the old registered uninstaller elevated (runas) and silent (/S). It is
      ; the only UAC prompt in the per-user lifetime. Best-effort: no _?= so the
      ; uninstaller self-copies and fully removes the old Program Files tree, its
      ; HKLM PATH segment and registration — a declined prompt just leaves the old
      ; copy and cannot fail this install. Silent uninstall preserves app data, and
      ; amenbo's store lives under its own app-data (not the Tauri identifier's), so
      ; user data is untouched either way.
      ExecShell "runas" "$2" "/S" SW_HIDE
    ${EndIf}
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
