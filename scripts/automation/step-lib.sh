# shellcheck shell=bash
# step-lib.sh — sourced helpers shared by the script steps: reading an input, leaving by a way out, and
# opening a pull request, pushing a tag and watching the CI runs that follow. Not executable on its own.
#
# A script step reads its inputs from the JSON file AMENBO_INPUT names and writes its way out to the
# one AMENBO_OUTPUT names, as {"version":1,"exit":…,"outs":{…},"report":…}. The way out is decided by
# that file, never by the exit code: anything a step cannot read or do ends it with `die`, which leaves
# through the error way out with the reason as its report.
#
# A file a step hands on is written beside output.json, and named in its outs by its file name.

# Stop the step for a reason: leave through the error way out ("*") with the reason as its report, and
# say it on stderr too. Without AMENBO_OUTPUT, or when output.json cannot be written, it ends with exit
# code 1, the reason being the last line on stderr. Called inside $(…) or a pipe, it stops the step's
# own shell as well, by a signal the traps below turn into the same exit code, so nothing after it
# runs and writes output.json over the reason.
die() {
    local code=1
    echo "✗ $*" >&2
    if [ -n "${AMENBO_OUTPUT:-}" ] &&
        jq -n --arg report "$*" '{version: 1, exit: "*", outs: {}, report: $report}' > "$AMENBO_OUTPUT"; then
        code=0
    fi
    if [ "$BASH_SUBSHELL" -gt 0 ]; then
        if [ "$code" = 0 ]; then kill -USR1 $$; else kill -USR2 $$; fi
    fi
    exit "$code"
}
trap 'exit 0' USR1
trap 'exit 1' USR2

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "スクリプトのステップとして動かしてください：AMENBO_INPUT と AMENBO_OUTPUT が設定されていません"

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
            die "リポジトリがわかりません：AMENBO_CI_REPO=OWNER/REPO を設定してください"
    fi
}

# The id of the run watch-ci.sh resolves for a mode, or the step stops with watch-ci.sh's reason.
# Exit code 3, from `main`, is handed back to the caller as it is.
resolve_run() {
    local id code=0
    id=$(AMENBO_CI_REPO="$repo" "$ci_watch" --print-id "$@") || code=$?
    [ "$code" = 0 ] || return "$code"
    [[ "$id" =~ ^[0-9]+$ ]] || die "watch-ci.sh --print-id $* が run の id を返しませんでした：\"$id\""
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
                die "run の結果を読めませんでした：$(run_url "$id")"
            case "$verdict" in
                "completed success" | "completed null" | "completed ")
                    die "watch-ci.sh は赤と言いましたが、run は \"$verdict\" です：$(run_url "$id")" ;;
                completed\ *) echo red ;;
                *) die "run が終わる前に監視が終わりました（\"$verdict\"）：$(run_url "$id")" ;;
            esac
            ;;
        7) die "run $id は completed ですが、結果の出ていないジョブがあります：$(run_url "$id")" ;;
        *) die "watch-ci.sh run $id が終了コード $code で終わりました：$(run_url "$id")" ;;
    esac
}

run_url() { echo "https://github.com/$repo/actions/runs/$1"; }

# The jobs of a run that did not end green or skipped, joined by ", ". It is read for a report, so a
# failed read says so in its place rather than stopping the step.
failed_jobs() {
    gh run view "$1" -R "$repo" --json jobs \
        --jq '[.jobs[] | select(.conclusion != "success" and .conclusion != "skipped") | "\(.name) (\(.conclusion))"] | join(", ")' ||
        echo "（run $1 のジョブを読めませんでした）"
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

# The release version an input carries, without the leading `v`, or the step stops.
input_version() {
    local version
    version=$(input "${1:-版}")
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || die "入力にバージョンがありません：\"$version\""
    echo "$version"
}

# The folder the release's artifacts were downloaded to, as an input carries it, or the step stops.
input_place() {
    local place
    place=$(input "${1:-置き場}")
    [ -n "$place" ] && [ -d "$place" ] || die "入力に、成果物をダウンロードしたフォルダがありません：\"$place\""
    echo "$place"
}

# Leave by the red way out, handing on the evidence file already written beside output.json under the
# output named RED_OUTPUT, which the verification steps share.
RED_EXIT="赤"
RED_OUTPUT="結果"
leave_red() {
    leave "$RED_EXIT" "$(jq -n --arg k "$RED_OUTPUT" --arg f "$1" '{($k): $f}')" "$2"
}
