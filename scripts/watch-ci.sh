#!/usr/bin/env bash
# watch-ci.sh — watch one CI run (or one pull request) and print only what changes.
#
# Emits a line when a check fails, when a pull request needs a hand, and when the
# thing being watched reaches a verdict — then exits. Nothing else. That makes it
# something to hand to a background watcher once, rather than a loop to poll.
#
# Exit code is the verdict: 0 = green, 1 = red, or the watch itself broke and said so.
#
# Usage — one mode per KIND of CI, so the caller names the thing and not the filter:
#
#   watch-ci.sh pr <number>                  a pull request: its checks, and its landing
#   watch-ci.sh main <sha>                   the run a merge into main started
#   watch-ci.sh tag <tag>                    the run a release tag started
#   watch-ci.sh dispatch <workflow> <ref> [not-before]
#                                            a run started by hand
#   watch-ci.sh run <id>                     a run by id, when you already have one
#
#   watch-ci.sh --print-id <mode> <args>     print the id it resolved to, and stop
#
# --print-id is for the caller that needs the run itself and not only its verdict —
# downloading its artifacts, say. Resolving it here rather than there is the point:
# an id picked by hand is picked from the same ambiguity described below.
#
# The mode is what carries the filter. A run is NOT addressed by id from the outside,
# because choosing the id is the part that goes wrong: one commit carries runs from
# several workflows and several events, and the newest of them is routinely a
# skipped bookkeeping run that would be reported as this commit's verdict. So the
# modes above resolve the id themselves, from the commit and the workflow together,
# and they refuse to guess: no run yet means wait and say so, more than one means
# stop rather than pick.
#
# Progress and diagnostics go to stderr throughout. Stdout carries the id under
# --print-id and the emitted events otherwise, so a caller may read either.
#
#   AMENBO_CI_REPO — OWNER/REPO to watch (default: the repository of the CWD)
#   AMENBO_CI_POLL, AMENBO_CI_APPEAR, AMENBO_CI_APPEAR_LIMIT, AMENBO_CI_MISS_LIMIT
#                  — the waits, in seconds and in tries (see their defaults below)
#
# The repository is resolved once, up front, and passed to every call afterwards, so
# nothing in the loop reads the filesystem. A watch that outlives the directory it
# was started in — a worktree folded while it runs — keeps working.
set -euo pipefail

# Overridable so a caller can tighten the wait, and so these paths can be exercised
# without sitting out the real intervals.
POLL_SECONDS="${AMENBO_CI_POLL:-45}"       # between checks of something already found
APPEAR_SECONDS="${AMENBO_CI_APPEAR:-10}"   # between checks for a run not registered yet
APPEAR_LIMIT="${AMENBO_CI_APPEAR_LIMIT:-30}"  # give up waiting for it to appear after this many
MISS_LIMIT="${AMENBO_CI_MISS_LIMIT:-3}"    # consecutive failed calls before calling the watch broken

usage() { awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; }

die() { echo "✗ $*" >&2; exit 1; }

print_id=""
if [ "${1:-}" = "--print-id" ]; then print_id=yes; shift; fi

mode="${1:-}"
case "$mode" in "" | -h | --help) usage; [ -n "$mode" ] || exit 2; exit 0 ;; esac
shift

# Print the id and stop, or watch it — every mode that resolves one ends here.
deliver() {
    if [ -n "$print_id" ]; then echo "$1"; else watch_run "$1"; fi
}

if [ -n "${AMENBO_CI_REPO:-}" ]; then
    repo="$AMENBO_CI_REPO"
else
    repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner) ||
        die "no repository: run this inside a checkout, or set AMENBO_CI_REPO=OWNER/REPO"
fi

# Find the one run matching a set of `gh run list` filters, and print its id.
# Its stdout carries the id and nothing else — the caller reads it through a command
# substitution, so a stray progress line there would be taken for part of the id.
#
# The two ways this can fail to be one run are opposite, so they are answered
# differently. None yet is normal right after a push — the run is registered a moment
# later — so it waits, out loud. More than one means the filters do not name a single
# run, and no rule for picking among them would be better than saying so: watching
# the wrong one reports a verdict that was never about this change.
resolve_one() {
    local label="$1"; shift
    local tries=0 rows count
    while :; do
        rows=$(gh run list -R "$repo" "$@" --json databaseId,name,event,createdAt) ||
            die "could not list runs for $label"
        count=$(jq 'length' <<< "$rows")
        if [ "$count" = 1 ]; then
            jq -r '.[0].databaseId' <<< "$rows"
            return 0
        fi
        if [ "$count" -gt 1 ]; then
            echo "✗ $label matches $count runs — name one of them with \`run <id>\`:" >&2
            jq -r '.[] | "    \(.databaseId)  \(.event)  \(.createdAt)  \(.name)"' <<< "$rows" >&2
            exit 1
        fi
        tries=$((tries + 1))
        [ "$tries" -ge "$APPEAR_LIMIT" ] &&
            die "$label has started no run after $((APPEAR_LIMIT * APPEAR_SECONDS))s"
        [ "$tries" = 1 ] && echo "waiting for $label to start a run" >&2
        sleep "$APPEAR_SECONDS"
    done
}

# The newest dispatched run of a workflow on a ref, delivered once it is the right one.
#
# A dispatched workflow keeps every run it has ever had on the same ref, so no filter
# names just this one, and the newest is the only candidate. That is a race against
# the dispatch that was just sent: until GitHub registers it, the newest run is the
# PREVIOUS one, whose verdict is about code that is no longer being asked about.
#
# So `not-before` closes it. Given an ISO-8601 UTC stamp taken before the dispatch,
# a newest run older than that is not this one, and it waits instead. ISO-8601 UTC
# sorts as text, so the comparison needs no date arithmetic. Without the stamp the
# newest is taken as-is, and its start time is printed for the caller to judge.
dispatch_newest() {
    local workflow="$1" ref="$2" not_before="$3" tries=0 row id created
    while :; do
        row=$(gh run list -R "$repo" --workflow "$workflow" --branch "$ref" \
            --event workflow_dispatch --limit 1 --json databaseId,createdAt \
            --jq '.[0] | "\(.databaseId) \(.createdAt)"') ||
            die "could not list runs of $workflow on $ref"
        id="${row%% *}"; created="${row##* }"
        if [ -n "$row" ] && { [ -z "$not_before" ] || [ ! "$created" \< "$not_before" ]; }; then
            echo "dispatched run of $workflow on $ref: $id (started $created)" >&2
            deliver "$id"
            return $?
        fi
        tries=$((tries + 1))
        [ "$tries" -ge "$APPEAR_LIMIT" ] &&
            die "$workflow on $ref started no run after $((APPEAR_LIMIT * APPEAR_SECONDS))s"
        [ "$tries" = 1 ] && echo "waiting for $workflow on $ref to start a run" >&2
        sleep "$APPEAR_SECONDS"
    done
}

# Watch one run to its conclusion. Failing jobs are named as they appear, each once.
watch_run() {
    local id="$1" miss=0 prev="" st fail status
    echo "watching run $id  https://github.com/$repo/actions/runs/$id"
    while :; do
        if ! st=$(gh run view "$id" -R "$repo" --json status,conclusion,jobs 2>&1); then
            miss=$((miss + 1))
            # One failed call is the network; three in a row is this watch being
            # over. Saying so is the whole point — a watcher that retries forever
            # looks exactly like one that is still watching.
            [ "$miss" -ge "$MISS_LIMIT" ] && { echo "watch broken: $st"; return 1; }
            sleep "$POLL_SECONDS"
            continue
        fi
        miss=0
        # `skipped` and `neutral` are not failures — a path filter deciding a job has
        # nothing to do is the normal shape of a run here, and reading those as red
        # would make every run red. `cancelled` and `timed_out` are not successes
        # either, and a watch that only looks for `failure` waits out both.
        fail=$(jq -r '.jobs[]
            | select(.conclusion == "failure" or .conclusion == "cancelled" or .conclusion == "timed_out")
            | "FAIL \(.name) (\(.conclusion))"' <<< "$st" | sort)
        [ -n "$fail" ] && [ "$fail" != "$prev" ] && echo "$fail"
        prev=$fail
        status=$(jq -r .status <<< "$st")
        if [ "$status" = completed ]; then
            local conclusion; conclusion=$(jq -r .conclusion <<< "$st")
            echo "run $id: $conclusion"
            # A re-run reopens the same id, which this has already left. Start
            # another watch for it rather than expecting this one to notice.
            [ "$conclusion" = success ] && return 0 || return 1
        fi
        sleep "$POLL_SECONDS"
    done
}

# Watch a pull request until it lands. Three things are worth a line: a check that
# failed, a merge that now needs a hand, and the landing itself.
watch_pr() {
    local pr="$1" miss=0 prevfail="" prevms="" v state ms fail
    echo "watching pull request $pr  https://github.com/$repo/pull/$pr"
    while :; do
        if ! v=$(gh pr view "$pr" -R "$repo" --json state,mergeStateStatus 2>&1); then
            miss=$((miss + 1))
            [ "$miss" -ge "$MISS_LIMIT" ] && { echo "watch broken: $v"; return 1; }
            sleep "$POLL_SECONDS"
            continue
        fi
        miss=0
        state=$(jq -r .state <<< "$v")
        [ "$state" != OPEN ] && { echo "pull request $pr: $state"; [ "$state" = MERGED ]; return $?; }
        ms=$(jq -r .mergeStateStatus <<< "$v")
        # DIRTY is the only merge state that will not clear on its own.
        [ "$ms" = DIRTY ] && [ "$ms" != "$prevms" ] && echo "needs a hand: $ms"
        prevms=$ms
        # `cancel` is reported alongside `fail`: a cancelled required check leaves the
        # merge waiting on something that will never arrive, which is indistinguishable
        # from still running unless it is named.
        fail=$(gh pr checks "$pr" -R "$repo" --json name,bucket 2>/dev/null |
            jq -r '.[] | select(.bucket == "fail" or .bucket == "cancel") | "FAIL \(.name) (\(.bucket))"' | sort)
        [ -n "$fail" ] && [ "$fail" != "$prevfail" ] && echo "$fail"
        prevfail=$fail
        sleep "$POLL_SECONDS"
    done
}

case "$mode" in
    pr)
        [ $# -eq 1 ] || die "usage: watch-ci.sh pr <number>"
        # A pull request is not a run, so there is no id to hand back.
        [ -z "$print_id" ] || die "--print-id takes a run, not a pull request"
        watch_pr "$1"
        ;;
    main)
        [ $# -eq 1 ] || die "usage: watch-ci.sh main <sha>"
        # Assigned, then watched. resolve_one runs in a command substitution, so its
        # own exit ends only that subshell — inlined as an argument, a refusal to guess
        # would be passed on as if it were an id.
        id=$(resolve_one "main $1" --commit "$1" --event push --workflow ci-change.yml)
        deliver "$id"
        ;;
    tag)
        [ $# -eq 1 ] || die "usage: watch-ci.sh tag <tag>"
        sha=$(git rev-parse "$1^{commit}" 2>/dev/null) || die "no such tag here: $1"
        id=$(resolve_one "tag $1" --commit "$sha" --event push --workflow release-tag.yml)
        deliver "$id"
        ;;
    dispatch)
        [ $# -ge 2 ] && [ $# -le 3 ] || die "usage: watch-ci.sh dispatch <workflow> <ref> [not-before]"
        dispatch_newest "$1" "$2" "${3:-}"
        ;;
    run)
        [ $# -eq 1 ] || die "usage: watch-ci.sh run <id>"
        deliver "$1"
        ;;
    *)
        die "unknown mode: $mode (try --help)"
        ;;
esac
