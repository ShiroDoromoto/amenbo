# shellcheck shell=bash
# step-lib.sh — sourced helpers shared by the script steps: reading an input, leaving by a way out, and
# opening a pull request, pushing a tag and watching the CI runs that follow. Not executable on its own.
#
# A script step reads its inputs from the JSON file AMENBO_INPUT names and writes its way out to the
# one AMENBO_OUTPUT names, as {"version":1,"exit":…,"outs":{…},"report":…}. The way out is decided by
# that file, never by the exit code: anything a step cannot read or do ends it with exit code 1, which
# the run leaves through the step's error way out, the reason being the last line on stderr.
#
# A file a step hands on is written beside output.json, and named in its outs by its file name.

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

# shellcheck disable=SC2034 # read by the steps that source this file
out_dir=$(dirname "$AMENBO_OUTPUT")

# One input of the step, by its name, or nothing.
input() { jq -r --arg k "$1" '.ins[$k] // empty' "$AMENBO_INPUT"; }

# Write output.json and stop: the way out, its outputs as a JSON object, and the report.
leave() {
    jq -n --arg exit "$1" --argjson outs "$2" --arg report "$3" \
        '{version: 1, exit: $exit, outs: $outs, report: $report}' > "$AMENBO_OUTPUT"
    exit 0
}

# The repository `gh` and watch-ci.sh are pointed at: AMENBO_CI_REPO, as in watch-ci.sh, or the
# repository of the current folder.
resolve_repo() {
    if [ -n "${AMENBO_CI_REPO:-}" ]; then
        repo="$AMENBO_CI_REPO"
    else
        repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner) ||
            die "no repository: set AMENBO_CI_REPO=OWNER/REPO"
    fi
}

# The id of the run watch-ci.sh resolves for a mode, or the step stops with watch-ci.sh's reason.
# Exit code 3, from `main`, is handed back to the caller as it is.
resolve_run() {
    local id code=0
    id=$(AMENBO_CI_REPO="$repo" "$ci_watch" --print-id "$@") || code=$?
    [ "$code" = 0 ] || return "$code"
    [[ "$id" =~ ^[0-9]+$ ]] || die "watch-ci.sh --print-id $* gave no run id: \"$id\""
    echo "$id"
}

# Wait for one run, and say how it ended: green, red, or the step stops. A run that read the same for
# AMENBO_CI_STALL seconds (watch-ci.sh's exit code 5) is only slow, so it is watched again. Exit code 1
# is also a watch that broke, so only a run that finished and did not succeed is called red.
watch_run() {
    local id="$1" code verdict
    while :; do
        code=0
        AMENBO_CI_REPO="$repo" "$ci_watch" run "$id" >&2 || code=$?
        [ "$code" = 5 ] || break
        echo "run $id read the same for a while; watching it again" >&2
    done
    case "$code" in
        0) echo green ;;
        1)
            verdict=$(gh run view "$id" -R "$repo" --json status,conclusion --jq '"\(.status) \(.conclusion)"') ||
                die "could not read the verdict of $(run_url "$id")"
            case "$verdict" in
                "completed success" | "completed null" | "completed ")
                    die "watch-ci.sh said red, and the run reads \"$verdict\": $(run_url "$id")" ;;
                completed\ *) echo red ;;
                *) die "the watch ended before the run did (\"$verdict\"): $(run_url "$id")" ;;
            esac
            ;;
        7) die "run $id reads completed, but a job it carries never reached a conclusion: $(run_url "$id")" ;;
        *) die "watch-ci.sh run $id ended with exit code $code: $(run_url "$id")" ;;
    esac
}

run_url() { echo "https://github.com/$repo/actions/runs/$1"; }

# The jobs of a run that did not end green or skipped, joined by ", ". It is read for a report, so a
# failed read says so in its place rather than stopping the step.
failed_jobs() {
    gh run view "$1" -R "$repo" --json jobs \
        --jq '[.jobs[] | select(.conclusion != "success" and .conclusion != "skipped") | "\(.name) (\(.conclusion))"] | join(", ")' ||
        echo "(could not read the jobs of run $1)"
}

# What a red run leaves to go on: its id and URL, the jobs that failed, and their failed log.
write_red_run() {
    local id="$1"
    {
        echo "run: $id"
        echo "url: $(run_url "$id")"
        echo "failed jobs: $(failed_jobs "$id")"
        echo
        gh run view "$id" -R "$repo" --log-failed || echo "(could not read the failed log of run $id)"
    }
}

# An ISO-8601 UTC stamp, taken before a dispatch, for watch-ci.sh's `dispatch … <not-before>`.
now_utc() { date -u +%Y-%m-%dT%H:%M:%SZ; }

ci_watch="$(pwd)/scripts/watch-ci.sh"
