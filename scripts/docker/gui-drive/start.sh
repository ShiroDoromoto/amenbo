#!/usr/bin/env bash
# start.sh — stand the room up and leave it standing.
#
# The unattended check beside this one (gui-e2e.sh) runs a road and exits. This
# one does not judge anything: it brings up a display, a session bus, a bound
# folder and the app, and then waits, so that a person or an agent can press
# things through `docker exec` for as long as they need.
#
# The session bus is put on a fixed socket rather than wrapped around the app
# with dbus-run-session, because every later `docker exec` has to reach the same
# bus: that is where the file manager door is answered, and a second bus would
# answer none of it.
set -euo pipefail

mkdir -p /out /work "$AMENBO_HOME"
: > /out/opened.log

echo "== start Xvfb"
Xvfb "$DISPLAY" -screen 0 1400x900x24 >/tmp/xvfb.log 2>&1 &
for _ in $(seq 20); do
  xdpyinfo -display "$DISPLAY" >/dev/null 2>&1 && break
  sleep 0.5
done
xdpyinfo -display "$DISPLAY" >/dev/null || { echo "✗ no X display"; cat /tmp/xvfb.log; exit 1; }
echo "  X display up on $DISPLAY"

echo "== start the session bus"
rm -f "${DBUS_SESSION_BUS_ADDRESS#unix:path=}"
dbus-daemon --session --address="$DBUS_SESSION_BUS_ADDRESS" --nofork >/tmp/dbus.log 2>&1 &
sleep 1
echo "  bus at $DBUS_SESSION_BUS_ADDRESS"

echo "== seed a bound folder with a file to open"
cd /work
printf '# notes\n\nA file for the doors out of the app to carry.\n' > /work/notes.md
"$AMENBO_CLI" init --name Alice --actor ai
# A folder that has never answered whether its AI may be started on Amenbo is
# asked on the way in, and that question sits over everything that is pressed
# here. A `no` closes it: it only means Amenbo stops asking.
"$AMENBO_CLI" agent-hook answer no --actor ai
# Filed and then finished, because a task still being created is listed but not
# yet a card anyone can act on, and a card is what there is to right-click.
"$AMENBO_CLI" task add --title "A task to right-click beside" --actor ai
"$AMENBO_CLI" task finish-creating 1 --actor ai

echo "== start the GUI"
amenbo-app >/tmp/gui.log 2>&1 &
GUI_PID=$!
sleep 20
if ! kill -0 "$GUI_PID" 2>/dev/null; then
  echo "✗ the GUI exited before it could be driven"; cat /tmp/gui.log; exit 1
fi
xwininfo -root -children -display "$DISPLAY" | grep -i webkit >/dev/null \
  || { echo "✗ no webview window"; cat /tmp/gui.log; exit 1; }
import -display "$DISPLAY" -window root /out/0-started.png

echo "→ the room is up (see /out/0-started.png). Press it with: shot / click / rclick / type / key / log"
wait "$GUI_PID"
