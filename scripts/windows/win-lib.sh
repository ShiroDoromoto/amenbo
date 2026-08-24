# shellcheck shell=bash
# win-lib.sh — sourced helpers shared by the Windows drive scripts (win-ps.sh,
# win-session1.sh, win-drive.sh). Not executable on its own.
#
# These exist because the app's escape hatches — "open with", "reveal in folder",
# "open in the default app" — can only be walked on a real Windows desktop, and
# `ssh win` does not reach one: it lands in session 0, elevated, where no GUI is
# drawn — measured, not theoretical. Everything here is about getting a script from
# this machine into the interactive session 1 of that one, and its output back.
#
# The machine is named by ssh config alias, not by address:
#   AMENBO_WIN_HOST — the ssh host to drive (default: win)
#   AMENBO_WIN_DIR  — the working dir on that machine, in Windows spelling
#                     (default: <the remote %USERPROFILE%>\amenbo-drive)
#
# Nothing here installs anything on the Windows side. The working dir holds only
# the .ps1 files these scripts push, the plan they were given, and whatever the
# run wrote; `win_clean_remote` takes it back out.

# Print a script's own leading comment block as its usage text. Read out of the file
# rather than written twice: a usage message that is a second copy of the header is
# the copy that goes stale.
win_usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$1"
}

# The ssh host these scripts drive.
win_host() { printf '%s' "${AMENBO_WIN_HOST:-win}"; }

# OpenSSH prints a post-quantum advisory on every connection to a server that does
# not offer one. It is not this script's output and it is not an error, so it is
# dropped — and only these exact lines are, so a real ssh failure still speaks.
win_strip_ssh_notes() {
    grep -v -E '^\*\* (WARNING: connection is not using|This session may be vulnerable|The server may need)' || true
}

# Run a PowerShell script (a file, or `-` for stdin) on the Windows machine, and
# relay its output here.
#
# The command is handed over as `-EncodedCommand`: UTF-16LE, base64. A plain
# `ssh win "powershell -Command …"` is quoted three times over — by this shell, by
# ssh's remote shell, and by PowerShell's own parser — so any script with a quote
# or a backslash in it arrives mangled. base64 has no metacharacters, so nothing
# in the script can be read as syntax on the way. The preamble it is wrapped in
# pins the output encoding to UTF-8; without it the reply comes back in the
# machine's ANSI code page (Shift-JIS on a Japanese install) and every non-ASCII
# character is destroyed by the time it reaches this terminal.
#
# stderr is folded into stdout: what the remote writes to either is equally the
# answer here, and keeping them apart would only split a PowerShell error away
# from the line that caused it.
win_ps() {
    local src="${1:--}" body enc pre
    if [ "$src" = "-" ]; then body=$(cat); else body=$(cat "$src"); fi
    # shellcheck disable=SC2016  # PowerShell's $-variables, quoted so this shell leaves them alone
    pre='[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; $ErrorActionPreference="Continue"; $ProgressPreference="SilentlyContinue";'
    enc=$(printf '%s\n%s' "$pre" "$body" | iconv -f UTF-8 -t UTF-16LE | base64 | tr -d '\n')
    ssh -o LogLevel=ERROR "$(win_host)" \
        "powershell -NoProfile -NonInteractive -EncodedCommand $enc" 2>&1 | win_strip_ssh_notes
}

# The working dir on the Windows machine, in Windows spelling (`C:\Users\…`).
# Asked of the machine itself rather than guessed, since the account name is not
# this repository's to know. The answer is cached for the life of the process.
_AMENBO_WIN_DIR=""
win_dir() {
    if [ -n "${AMENBO_WIN_DIR:-}" ]; then printf '%s' "$AMENBO_WIN_DIR"; return; fi
    if [ -z "$_AMENBO_WIN_DIR" ]; then
        # shellcheck disable=SC2016  # $env: is PowerShell's, evaluated on the far machine
        _AMENBO_WIN_DIR=$(printf '%s' '$env:USERPROFILE' | win_ps - | tr -d '\r' | head -n 1)
        [ -n "$_AMENBO_WIN_DIR" ] || { echo "✗ could not read %USERPROFILE% from $(win_host)" >&2; return 1; }
        _AMENBO_WIN_DIR="$_AMENBO_WIN_DIR\\amenbo-drive"
    fi
    printf '%s' "$_AMENBO_WIN_DIR"
}

# The same dir spelled for scp, which speaks a URL and so takes forward slashes.
# shellcheck disable=SC1003  # '\\' is one literal backslash for tr, not an escaped quote
win_dir_scp() { win_dir | tr '\\' '/'; }

# Push files into the working dir, creating it first. Every run pushes the .ps1
# files again rather than trusting what is there: they are a few KB, and a stale
# copy on the far side is a failure that looks like a bug in the change under test.
#
# A .ps1 gets a UTF-8 byte-order mark on the way. Windows PowerShell 5.1 — the one
# that is always installed, and the one these scripts are run by — reads a script
# file as the machine's ANSI code page unless a BOM says otherwise, so on a Japanese
# install every non-ASCII character in a script written here (an em dash in a
# comment is enough) arrives as mojibake and the parse fails on a string that no
# longer terminates. scp copies bytes and changes nothing, so the mark has to be put
# on this side.
win_push() {
    local dir stage f; dir=$(win_dir) || return 1
    printf '%s' "New-Item -ItemType Directory -Force -Path '$dir' | Out-Null" | win_ps - >/dev/null
    stage=$(mktemp -d -t amenbo-win-push)
    for f in "$@"; do
        case "$f" in
            *.ps1) { printf '\357\273\277'; cat "$f"; } > "$stage/$(basename "$f")" ;;
            *) cp "$f" "$stage/$(basename "$f")" ;;
        esac
    done
    scp -q -o LogLevel=ERROR "$stage"/* "$(win_host):$(win_dir_scp)/"
    rm -rf "$stage"
}

# Push a local script and run it in the interactive desktop session, through
# session1.ps1. Extra files the script needs are pushed by the caller beforehand;
# this pushes the runner and the script itself.
#   win_session1 <local-script> [timeout-seconds]
win_session1() {
    local script="$1" timeout="${2:-120}" dir
    dir=$(win_dir) || return 1
    win_push scripts/windows/session1.ps1 "$script"
    printf '%s\n' \
        "& '$dir\\session1.ps1' -Script '$dir\\$(basename "$script")' -TimeoutSec $timeout -WorkDir '$dir'" |
        win_ps -
}

# Remove the working dir from the Windows machine, and any scheduled task left
# behind by an interrupted run.
win_clean_remote() {
    local dir; dir=$(win_dir) || return 1
    printf '%s\n' \
        "Get-ScheduledTask -TaskName 'amenbo-session1*' -ErrorAction SilentlyContinue | Unregister-ScheduledTask -Confirm:\$false" \
        "Remove-Item -Recurse -Force '$dir' -ErrorAction SilentlyContinue" \
        "'cleaned $dir'" | win_ps -
}
