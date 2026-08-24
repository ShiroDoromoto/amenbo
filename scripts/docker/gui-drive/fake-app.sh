#!/usr/bin/env bash
# fake-app.sh — the far side of a door out of the app.
#
# A screenshot can only say that the menu closed; what opened is on the other
# side of the handover, in a process the app does not own. So every application
# this container offers is this one script under a different name: it records the
# arguments it was handed and exits. A door that fired is then a line in the log,
# with the file it carried, and a door that silently did nothing leaves none.
#
# Usage: fake-app.sh <label> [argument...]   (wired up by the .desktop entries)
set -euo pipefail

label="${1:?a label is required}"
shift

printf '%s\t%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$label" "$*" >> /out/opened.log
