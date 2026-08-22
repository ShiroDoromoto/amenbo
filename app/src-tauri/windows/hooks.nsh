; hooks.nsh — Tauri NSIS installer hooks for amenbo.
;
; The unified installer ships the amenbo CLI as a Tauri sidecar (externalBin);
; NSIS installs it into $INSTDIR under the stem that config names, next to the
; GUI. These hooks put $INSTDIR on the *user* PATH so the CLI works from any
; terminal — one installer lands GUI + CLI on PATH.
;
; The stem is this build's own name: `amenbo.exe` for the release, and
; `amenbo-dev.exe` / `amenbo-dev-<theme>.exe` for a development build (the
; Makefile's GUI_DEV_CONFIG). It has to be — every channel installs into a
; directory of its own and puts that directory on PATH, so a shared name meant
; `where amenbo` listed production and every preview a member had taken, and the
; one a shell resolved was whichever had been installed first (AMB-T-3504).
; Nothing here reads the name: the hooks work on $INSTDIR, and each build's
; directory is already its own.
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

; ---------------------------------------------------------------------------
; PATH is a ";"-separated list, so both hooks work on it a *whole segment* at a
; time. A substring test cannot: $INSTDIR is `%LOCALAPPDATA%\<productName>` and
; productName is channel-specific, so a theme preview's `…\amenbo (dev 3497)`
; contains the release build's `…\amenbo` — and any PATH entry can contain
; another (`…\foo` inside `…\foo\tools`). Measured on Windows 11 with a
; substring implementation (`AMB-T-3497`): the append silently did nothing
; because it "found" its own directory inside a longer one, and the removal
; clipped the *neighbouring* entry it had matched inside, turning it into a
; path that does not exist — with no error either way.
;
; AmenboPathDropSegment rebuilds PATH without one directory, and reports
; whether it was there. Both hooks need exactly that: the installer to decide
; whether to append, the uninstaller to take its own entry back out.
;
; In:   top of stack = the PATH string, below it = the directory to drop
; Out:  top of stack = PATH without that segment (every occurrence),
;       below it = "1" if it was present, "" if not
; Empty segments are preserved; comparison is case-insensitive (StrCmp), which
; is what Windows paths want.
!macro AmenboPathDropSegment UN
Function ${UN}AmenboPathDropSegment
  Exch $R0        ; $R0 = PATH
  Exch
  Exch $R1        ; $R1 = the directory to drop
  Push $R2        ; what is left to scan
  Push $R3        ; the segment cut off its front (and the char being scanned)
  Push $R4        ; the rebuilt PATH, every kept segment prefixed with ";"
  Push $R5        ; "1" once our segment has been seen
  Push $R6        ; scan cursor

  StrCpy $R2 $R0
  StrCpy $R4 ""
  StrCpy $R5 ""

  loop:
    StrCmp $R2 "" done
    StrCpy $R6 0
    find:
      StrCpy $R3 $R2 1 $R6
      StrCmp $R3 "" last
      StrCmp $R3 ";" cut
      IntOp $R6 $R6 + 1
      Goto find
    last:                    ; no separator left — the rest is one segment
      StrCpy $R3 $R2
      StrCpy $R2 ""
      Goto have
    cut:
      IntCmp $R6 0 empty     ; StrCpy's maxlen 0 means "all of it", not "none"
      StrCpy $R3 $R2 $R6
      Goto advance
    empty:
      StrCpy $R3 ""
    advance:
      IntOp $R6 $R6 + 1
      StrCpy $R2 $R2 "" $R6
    have:
      StrCmp $R3 $R1 0 keep
      StrCpy $R5 "1"
      Goto loop
    keep:
      StrCpy $R4 "$R4;$R3"
      Goto loop
  done:
  StrCmp $R4 "" +2
    StrCpy $R4 $R4 "" 1      ; drop the ";" the first kept segment was given

  StrCpy $R0 $R4
  StrCpy $R1 $R5
  Pop $R6
  Pop $R5
  Pop $R4
  Pop $R3
  Pop $R2
  Exch $R1        ; leave the "was it there" flag
  Exch
  Exch $R0        ; leave the rebuilt PATH on top of it
FunctionEnd
!macroend

!insertmacro AmenboPathDropSegment ""
!insertmacro AmenboPathDropSegment "un."

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Registering amenbo CLI on PATH…"
  ReadRegStr $0 HKCU "Environment" "Path"
  ; Append $INSTDIR only if absent, so re-installs and in-place updates don't
  ; grow PATH with duplicates.
  Push "$INSTDIR"
  Push "$0"
  Call AmenboPathDropSegment
  Pop $2   ; PATH without our segment — not needed here, only the flag is
  Pop $1
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
  Push "$INSTDIR"
  Push "$0"
  Call un.AmenboPathDropSegment
  Pop $1   ; PATH without our segment
  Pop $2   ; whether it was there at all
  ; Only write when we actually had an entry — an install we never registered
  ; must not have its PATH rewritten on the way out.
  ${If} $2 != ""
    WriteRegExpandStr HKCU "Environment" "Path" "$1"
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
!macroend
