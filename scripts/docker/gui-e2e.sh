#!/usr/bin/env bash
# gui-e2e.sh — runs INSIDE the linux-gui-e2e container (see Dockerfile.linux-gui-e2e).
# Asks the one question the other suites can't: when a SEPARATE process writes to the
# store, does the running GUI's webview actually repaint?
#
# The card title this writes and looks for is NOT baked in here: it comes from the scenario
# source (verification/scenarios/), off the `steps_gui` road this check walks, resolved on
# the host by `make verify-gui-linux` through the `amenbo-scenario` crate and passed in as
# AMENBO_E2E_CARD — the container carries no toolchain to read the YAML itself. Edit that
# road and this Linux check follows.
#
# Screenshots land in the mounted /out (host: ./dist/gui-e2e) — 1-before.png,
# 2-after.png and their diff. The verdict is read off the picture by OCR, so this can run
# unattended (it does, in the Linux GUI bundle workflow): the card title the CLI wrote must
# be absent from the before shot and present in the after shot, with no interaction in
# between. A pixel diff alone would also go green on a repaint that carried nothing.
set -euo pipefail

# The title the CLI writes; OCR must find it only in the after shot. Sourced from the
# scenario, not hard-coded — fail loudly if the host launcher forgot to pass it.
CARD="${AMENBO_E2E_CARD:?AMENBO_E2E_CARD must be set (the scenario-derived card title, see make verify-gui-linux)}"

# Fold a string to a punctuation-insensitive, lower-case, single-spaced form. tesseract on
# a rendered card reads the words reliably but not every glyph (an em-dash comes back as a
# dash, a space, or nothing), so the title is matched on its alphanumerics, not verbatim.
norm() { tr '[:upper:]' '[:lower:]' | tr -c '[:alnum:]' ' ' | tr -s ' '; }
NEEDLE="$(printf '%s' "$CARD" | norm)"

# tesseract reads the screen line by line, straight across. A title too long for its card wraps,
# and whatever sits to the left of that second line — the sidebar — is read between the halves,
# so the title's words arrive in order but not adjacent. Match them as a subsequence: every word
# present, in order, with anything allowed in the gaps. A title the CLI never wrote still fails,
# since its words are not all up there to be found.
has_words() {
  local hay=" $1 " w
  for w in $2; do
    case "$hay" in
      *" $w "*) hay=" ${hay#*" $w "}" ;;
      *) return 1 ;;
    esac
  done
  return 0
}

export AMENBO_HOME=/root/amenbo-home   # a throwaway store; never the real app-data tree
export AMENBO_UPDATE_CHECK=0
export DISPLAY=:99
# No GPU in a container: keep WebKit off the compositing/dmabuf paths it can't take.
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
mkdir -p /out /work "$AMENBO_HOME"

echo "== seed the store (CLI, before the GUI starts)"
# `init` binds /work to a project, and that binding is what scopes every CLI call below: an AI does
# not name a project, so the writes carry no --project.
cd /work
amenbo init --name Alice --actor ai
# A store nobody has been welcomed into opens the GUI on first-run setup, and that dialog sits over
# the board — the card under test is then half behind it, and OCR truthfully reports only the sliver
# it can see. The question here is whether the board repaints, not whether a newcomer is greeted, so
# the store arrives already welcomed.
amenbo config set onboarded true --actor ai
amenbo task add --title "SEED TASK BEFORE GUI" --actor ai

echo "== start Xvfb"
Xvfb :99 -screen 0 1400x900x24 >/tmp/xvfb.log 2>&1 &
sleep 2
xdpyinfo -display :99 >/dev/null && echo "  X display up"

echo "== start the GUI"
dbus-run-session -- amenbo-app >/tmp/gui.log 2>&1 &
GUI_PID=$!
sleep 20
if ! kill -0 "$GUI_PID" 2>/dev/null; then
  echo "✗ the GUI exited before it could be looked at"; cat /tmp/gui.log; exit 1
fi
xwininfo -root -children -display :99 | grep -i webkit || { echo "✗ no webview window"; exit 1; }
import -display :99 -window root /out/1-before.png

echo "== external write from a separate CLI process (the thing under test)"
amenbo task add --title "$CARD" --actor ai
sleep 6
import -display :99 -window root /out/2-after.png

changed="$(compare -metric AE /out/1-before.png /out/2-after.png /out/3-diff.png 2>&1 || true)"
echo ""
echo "== pixels changed: $changed"
[ "$changed" != "0" ] || { echo "✗ the webview did not repaint"; exit 1; }

echo "== read the board back off the screen (OCR)"
tesseract /out/1-before.png /out/1-before 2>/dev/null
tesseract /out/2-after.png /out/2-after 2>/dev/null
before="$(norm < /out/1-before.txt)"
after="$(norm < /out/2-after.txt)"
if has_words "$before" "$NEEDLE"; then
  echo "✗ '$CARD' was on the board BEFORE the CLI wrote it — the check proves nothing"; exit 1
fi
if ! has_words "$after" "$NEEDLE"; then
  echo "✗ the webview repainted but the '$CARD' card is not on the board"
  echo "  (what OCR read:)"; cat /out/2-after.txt; exit 1
fi
echo "→ '$CARD' appeared on the board with no interaction (see /out/2-after.png)"
kill "$GUI_PID" 2>/dev/null || true
