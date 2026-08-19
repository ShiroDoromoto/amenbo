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

# Assembling those lines is also what breaks a glyph. With the sidebar's words joined onto the
# title's second line, the line's context bends a short `i` into an `l`: `me-ai` is read as `me-al`,
# every time, on the tesseract this image carries. Sparse mode (`--psm 11`) assembles no lines, so
# no such context exists and the glyph is read right — but it is documented as returning text "in no
# particular order", which is the one thing the subsequence match above rests on. So neither reading
# is trusted alone: read both ways, and let the ordered one stay primary.
read_board() {   # <png> <stem> — leaves <stem>-lines.txt and <stem>-sparse.txt beside it
  tesseract "$1" "$2-lines" 2>/dev/null
  tesseract "$1" "$2-sparse" --psm 11 2>/dev/null
}

# The card counts as on screen if EITHER reading finds it, which is what makes a glyph one of them
# fudged survivable. It costs nothing in strictness: the words of a title the CLI never wrote are in
# neither reading, so the before shot is held to both — absent there means absent twice over.
seen_in() {      # <stem> — 0 if the needle is in either reading
  has_words "$(norm < "$1-lines.txt")" "$NEEDLE" && return 0
  has_words "$(norm < "$1-sparse.txt")" "$NEEDLE"
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
# A folder that has never answered whether its AI may be started on amenbo is asked on the way in,
# and that question sits over the board the same way. A `no` is what closes it: it only means amenbo
# stops asking. Nothing here is about the wiring — a question left standing hides the card under test.
amenbo agent-hook answer no --actor ai
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
read_board /out/1-before.png /out/1-before
read_board /out/2-after.png /out/2-after
if seen_in /out/1-before; then
  echo "✗ '$CARD' was on the board BEFORE the CLI wrote it — the check proves nothing"; exit 1
fi
if ! seen_in /out/2-after; then
  echo "✗ the webview repainted but the '$CARD' card is not on the board"
  # Both readings, because which one lost the card is the first thing to look at: the ordered one
  # alone is what sent the last failure looking for a product fault that was not there.
  echo "  (what OCR read, line by line:)"; cat /out/2-after-lines.txt
  echo "  (and reading it as sparse text:)"; cat /out/2-after-sparse.txt
  exit 1
fi
echo "→ '$CARD' appeared on the board with no interaction (see /out/2-after.png)"
kill "$GUI_PID" 2>/dev/null || true
