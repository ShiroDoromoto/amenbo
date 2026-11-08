#!/usr/bin/env bash
# watch-ci.test.sh — exercise watch-ci.sh's `run` mode against a stand-in `gh`.
#
# Each case writes the answers `gh run view` gives, one file per call, and checks the exit code
# watch-ci.sh ends with. The stand-in answers its Nth call with call-N.json, and every call after
# the last file with the last one. Intervals are shrunk through the environment, so nothing here
# waits out the real ones.
#
# Usage: scripts/watch-ci.test.sh
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir "$work/bin"
cat > "$work/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
n=$(( $(cat "$GH_STUB/calls" 2>/dev/null || echo 0) + 1 ))
echo "$n" > "$GH_STUB/calls"
answer="$GH_STUB/call-$n.json"
# The files are numbered from 1 with no gaps, so the last one is call-<how many there are>.
[ -f "$answer" ] || answer="$GH_STUB/call-$(find "$GH_STUB" -name 'call-*.json' | wc -l | tr -d ' ').json"
cat "$answer"
EOF
chmod +x "$work/bin/gh"

failed=0

# run_case <name> <expected exit code> <expected least calls> — with the case's call-N.json in $GH_STUB.
run_case() {
    local name="$1" want="$2" least="$3" got calls
    set +e
    PATH="$work/bin:$PATH" AMENBO_CI_REPO=example/repo AMENBO_CI_POLL=0.1 \
        "$here/watch-ci.sh" run 1 > "$GH_STUB/out" 2> "$GH_STUB/err"
    got=$?
    set -e
    calls=$(cat "$GH_STUB/calls")
    if [ "$got" -eq "$want" ] && [ "$calls" -ge "$least" ]; then
        echo "ok   $name"
    else
        echo "FAIL $name: exit code $got (want $want), $calls calls (want at least $least)"
        sed 's/^/     | /' "$GH_STUB/out" "$GH_STUB/err"
        failed=1
    fi
}

# new_case — a fresh directory for the next case's answers.
new_case() {
    GH_STUB=$(mktemp -d "$work/case.XXXXXX")
    export GH_STUB
}

job() { printf '{"name": "%s", "status": "%s", "conclusion": %s}' "$1" "$2" "$3"; }
run() { printf '{"status": "%s", "conclusion": %s, "jobs": [%s]}' "$1" "$2" "$3"; }

# A finished run whose job GitHub reports as in_progress with a conclusion: the job is over.
new_case
run completed '"success"' "$(job "ci / detect changed paths" in_progress '"success"')" > "$GH_STUB/call-1.json"
run_case "a job with a conclusion is over, whatever its status says" 0 1

# The window right after a re-run: the run still reads completed, its job is queued with no
# conclusion. It is waited out, and the run's next verdict is the one reported.
new_case
for n in 1 2 3; do
    run completed '"failure"' "$(job build queued null)" > "$GH_STUB/call-$n.json"
done
run completed '"success"' "$(job build completed '"success"')" > "$GH_STUB/call-4.json"
run_case "a re-run's queued job is waited out" 0 4

# The same window that never closes ends once AMENBO_CI_RERUN_LIMIT runs out.
new_case
run completed '"success"' "$(job build queued null)" > "$GH_STUB/call-1.json"
AMENBO_CI_RERUN_LIMIT=1 run_case "a completed run that keeps a job with no conclusion stops" 7 2
grep -q "build: queued" "$GH_STUB/out" || { echo "FAIL the job left without a conclusion is not named"; failed=1; }

exit "$failed"
