#!/usr/bin/env bash
# helpers.sh — the six words this container is driven with.
#
# Driving a screen from outside the container is a `docker exec` per press, and
# the press itself is an xdotool line with a display, a mouse move and a button
# number on it. Wrapping each one here keeps the press at the front of the
# command, where the person reading the session can see what was pressed.
#
# One file, six names: /usr/local/bin/{shot,click,rclick,type,key,log} are
# symlinks to this, and the name it was invoked under chooses the branch. Reach
# them as `docker exec <container> click 300 240`, which runs the file directly.
# Through a shell (`sh -c 'type ...'`) the shell's own builtin wins instead, so
# spell the path out there.
#
#   shot [name]     screenshot the whole display into /out/<name>.png
#   click X Y       left button at X Y
#   rclick X Y      right button at X Y
#   type TEXT       type TEXT into whatever holds the keyboard focus
#   key NAME...     press keys by name (Return, Escape, ctrl+a)
#   log [-f]        what the doors out of the app have recorded so far
set -euo pipefail

case "$(basename "$0")" in
  shot)
    name="${1:-shot}"
    import -display "$DISPLAY" -window root "/out/$name.png"
    echo "→ /out/$name.png"
    ;;
  click)
    xdotool mousemove "${1:?an X coordinate is required}" "${2:?a Y coordinate is required}" click 1
    ;;
  rclick)
    xdotool mousemove "${1:?an X coordinate is required}" "${2:?a Y coordinate is required}" click 3
    ;;
  type)
    xdotool type --delay 40 "$*"
    ;;
  key)
    xdotool key "$@"
    ;;
  log)
    touch /out/opened.log
    if [ "${1:-}" = "-f" ]; then
      exec tail -f /out/opened.log
    fi
    cat /out/opened.log
    ;;
  *)
    echo "helpers.sh: invoke it as shot / click / rclick / type / key / log" >&2
    exit 2
    ;;
esac
