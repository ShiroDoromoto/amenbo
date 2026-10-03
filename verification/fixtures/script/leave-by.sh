#!/bin/sh
# A script step's program, told by its arguments how to end. What it writes is the file named in
# AMENBO_OUTPUT, in the shape {"version":1,"exit":…,"outs":{…},"report":…}; every other way of
# ending here is one Amenbo takes the step out of by the error way out.
#
#   leave <exit> [<output> <value>]  write output.json naming <exit>, with one output if given
#   end-with <code> [<file>]         take away <file> beside this program, then end with <code>
#   write-nothing                    end with 0 and write no output.json
#   write-garbled                    end with 0 having written something that is not JSON
#   wait <seconds>                   run that long, for a timeout to stop it
#
# The values are written into JSON as they are, so a road keeps quotes and backslashes out of them.

mode=$1
shift

case $mode in
leave)
    outs=""
    if [ $# -ge 3 ]; then
        outs="\"$2\":\"$3\""
    fi
    printf '{"version":1,"exit":"%s","outs":{%s},"report":"left by %s"}\n' "$1" "$outs" "$1" >"$AMENBO_OUTPUT"
    ;;
end-with)
    if [ $# -ge 2 ]; then
        /bin/rm -f "${0%/*}/$2"
    fi
    exit "$1"
    ;;
write-nothing)
    ;;
write-garbled)
    printf 'this is not JSON\n' >"$AMENBO_OUTPUT"
    ;;
wait)
    exec /bin/sleep "$1"
    ;;
*)
    echo "leave-by.sh: no mode named '$mode'" >&2
    exit 2
    ;;
esac
