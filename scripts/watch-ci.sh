#!/usr/bin/env bash
# watch-ci.sh — watch one CI run, one pull request, or one check of one, and print
# only what changes.
#
# Emits a line when a check fails, when a pull request needs a hand, and when the
# thing being watched reaches a verdict — then exits. Otherwise it speaks only to say
# it could not read what it is watching. That makes it something to hand to a
# background watcher once, rather than a loop to poll.
#
# Exit code is the verdict: 0 = green, 1 = red, or the watch itself broke and said so.
#
# Usage — one mode per KIND of CI, so the caller names the thing and not the filter:
#
#   watch-ci.sh pr <number>                  a pull request: its checks, and its landing
#   watch-ci.sh check <pr> <name>            one named check on a pull request, on its own
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
# an id picked by hand is picked from the same ambiguity described below. Neither `pr`
# nor `check` resolves a run, so neither takes it.
#
# `pr` and `check` differ in what they call the end. `pr` ends when the pull request
# lands, so one held open on purpose — a theme branch collecting its checks before
# anyone decides to merge it — can never satisfy it. `check` ends when the single check
# it was given settles, and leaves the pull request where it found it.
#
# The mode is what carries the filter. A run is NOT addressed by id from the outside,
# because choosing the id is the part that goes wrong: one commit carries runs from
# several workflows and several events, and the newest of them is routinely a
# skipped bookkeeping run that would be reported as this commit's verdict. So the
# modes above resolve the id themselves, from the commit and the workflow together,
# and they refuse to guess: no run yet means wait and say so, more than one means
# stop rather than pick.
#
# Waiting for something to appear is open-ended on purpose. How long GitHub takes to
# register a run, and how long a check takes to be created, are GitHub's to decide, and
# a threshold placed on a distribution the caller cannot see reads slowness as absence.
# So a wait ends on evidence that the thing is not coming — a commit or a tag that is
# not on the remote, a workflow that is not there, every run on the head commit finished
# without the check — and not on a clock. Until then it waits, and says so as it waits.
#
# Progress and diagnostics go to stderr throughout. Stdout carries the id under
# --print-id and the emitted events otherwise, so a caller may read either.
#
#   AMENBO_CI_REPO — OWNER/REPO to watch (default: the repository of the CWD)
#   AMENBO_CI_POLL, AMENBO_CI_APPEAR — how often it reads what it watches, in seconds
#   AMENBO_CI_MISS_LIMIT — consecutive failed calls before the watch is called broken
#   AMENBO_CI_APPEAR_LIMIT — rounds to wait for something to appear before giving up.
#                  Unset, there is no deadline and the evidence above is what ends it
#
# The repository is resolved once, up front, and passed to every call afterwards, so
# nothing in the loop reads the filesystem. A watch that outlives the directory it
# was started in — a worktree folded while it runs — keeps working.
set -euo pipefail

# Overridable so a caller can tighten the wait, and so these paths can be exercised
# without sitting out the real intervals.
POLL_SECONDS="${AMENBO_CI_POLL:-45}"       # between checks of something already found
APPEAR_SECONDS="${AMENBO_CI_APPEAR:-10}"   # between checks for a run not registered yet
APPEAR_LIMIT="${AMENBO_CI_APPEAR_LIMIT:-}" # rounds before giving up on it; empty = no deadline
MISS_LIMIT="${AMENBO_CI_MISS_LIMIT:-3}"    # consecutive failed calls before calling the watch broken

# How many rounds of the appear wait fit in a minute, so an open-ended one repeats what
# it is waiting for about that often. A watch that has gone quiet for ten minutes cannot
# be told from one that died, and saying it once at the start is that.
SAY_EVERY=$(( APPEAR_SECONDS > 0 ? (60 + APPEAR_SECONDS - 1) / APPEAR_SECONDS : 1 ))

usage() { awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; }

die() { echo "✗ $*" >&2; exit 1; }

# One round of waiting for something that has not appeared yet: say what is being waited
# for, then sleep. Deciding whether it is still coming belongs to the caller, which has
# the evidence; all this holds is the deadline a caller asked for by hand.
appear_wait() {
    local tries="$1" what="$2"
    [ -n "$APPEAR_LIMIT" ] && [ "$tries" -ge "$APPEAR_LIMIT" ] &&
        die "gave up waiting for $what after $((APPEAR_LIMIT * APPEAR_SECONDS))s"
    [ $(( (tries - 1) % SAY_EVERY )) -eq 0 ] && echo "waiting for $what" >&2
    sleep "$APPEAR_SECONDS"
}

# The commit a ref points at on the remote, or nothing and a non-zero exit. The commits
# endpoint resolves a sha, a branch and a tag alike, so one question covers what every
# mode below waits on, and asking it is what tells a ref that was mistyped from one
# whose run is merely slow to register — the difference a clock was standing in for.
#
# It answers with the full sha, which is also what `gh run list --commit` wants: a short
# one matches no run there, and would have been waited out as if none had started.
remote_ref_sha() { gh api "repos/$repo/commits/$1" --jq .sha 2>/dev/null; }

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

# Find the one run a push to a commit started under a workflow, and print its id.
# Its stdout carries the id and nothing else — the caller reads it through a command
# substitution, so a stray progress line there would be taken for part of the id.
#
# The two ways this can fail to be one run are opposite, so they are answered
# differently. None yet is normal right after a push — the run is registered a moment
# later — so it waits, out loud, for as long as that takes. More than one means the
# filters do not name a single run, and no rule for picking among them would be better
# than saying so: watching the wrong one reports a verdict that was never about this
# change.
#
# The wait has no deadline, so the caller resolves the commit on the remote first and
# hands the sha in — that answer, and not a clock, is what rules out a run that is never
# coming. Here the commit is known to exist, so nothing left is worth giving up on.
resolve_one() {
    local label="$1" sha="$2" workflow="$3"
    local tries=0 rows count
    while :; do
        rows=$(gh run list -R "$repo" --commit "$sha" --event push --workflow "$workflow" \
            --json databaseId,name,event,createdAt) ||
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
        appear_wait "$tries" "$label to start a run"
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
    # Both halves of what was named are asked about before the wait, for the reason
    # resolve_one gives: a workflow that is not on the remote and a ref that is not
    # either are the two ways this waits on a run nobody is ever going to start.
    gh workflow view "$workflow" -R "$repo" >/dev/null 2>&1 ||
        die "no workflow \"$workflow\" on $repo — nothing there will be dispatched"
    remote_ref_sha "$ref" >/dev/null ||
        die "no ref \"$ref\" on $repo — nothing there will be dispatched"
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
        appear_wait "$tries" "$workflow on $ref to start a run"
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

# Watch a pull request until it lands, or until one of its checks goes red. Three
# things are worth a line: a check that failed, a merge that now needs a hand, and the
# landing itself.
#
# A failed check ends the watch. A required check that is red does not clear on its
# own — the pull request cannot land until someone pushes a fix — so watching on is
# waiting for something that will not happen while nothing is said. Since the events
# reach a caller reading through a pipe only once the process ends, the red that was
# already detected would not even be shown. A re-run is what clears a flaky check, and
# starting another watch after it is the caller's, as it is for `run`.
watch_pr() {
    local pr="$1" miss=0 prevms="" prevck="" v state ms checks fail
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
        # `gh pr checks` refuses, rather than reporting nothing, when the head commit
        # carries no check yet — the window right after a push, which is exactly when
        # a watch gets started. So the call is allowed to fail and is read for what it
        # left behind: a JSON array is an answer, empty or not, and anything else is
        # nothing to go on this round. Saying so once, in the shape the broken watch
        # above uses, is what keeps this from being the silent death it was: the
        # assignment was killed by `pipefail` under `set -e`, and the reason for it
        # went to /dev/null.
        #
        # What ends the watch when the connection itself is gone stays `gh pr view`,
        # which fails the same way earlier in the same round and counts its misses.
        checks=$(gh pr checks "$pr" -R "$repo" --json name,bucket 2>&1) || :
        # `cancel` is reported alongside `fail`: a cancelled required check leaves the
        # merge waiting on something that will never arrive, which is indistinguishable
        # from still running unless it is named.
        if fail=$(jq -er 'map(select(.bucket == "fail" or .bucket == "cancel")
                | "FAIL \(.name) (\(.bucket))") | sort | join("\n")' <<< "$checks" 2>/dev/null); then
            prevck=""
            [ -n "$fail" ] && { echo "$fail"; return 1; }
        else
            [ "$checks" != "$prevck" ] && echo "checks not read: ${checks:-no output}"
            prevck=$checks
        fi
        sleep "$POLL_SECONDS"
    done
}

# Whether every run on a pull request's head commit has finished, printing why on the
# way out: the caller's message is about the check that is missing, not about the runs.
#
# Three answers, and only one of them ends a wait. Runs that have all completed mean the
# check is not coming — a check run is created by the job that reports it, so no job
# left to run is no check left to create. A run still going means it may yet arrive. No
# runs at all means the push was accepted and GitHub has registered nothing yet, which
# is the window a watch is most often started in, so that waits too. A call that fails
# answers nothing, and waiting is the safe way to be wrong about it.
#
# It reads Actions alone. A check reported by some other app is outside what this can
# see, and would be called absent while it was merely slow — no such check is required
# in this repository, and one would be a reason to widen this rather than to widen the
# wait it replaced.
head_runs_settled() {
    local pr="$1" sha rows
    sha=$(gh pr view "$pr" -R "$repo" --json headRefOid --jq .headRefOid 2>/dev/null) || return 1
    [ -n "$sha" ] || return 1
    rows=$(gh run list -R "$repo" --commit "$sha" --json status 2>/dev/null) || return 1
    [ "$(jq 'length' <<< "$rows")" -gt 0 ] || return 1
    [ "$(jq '[.[] | select(.status != "completed")] | length' <<< "$rows")" = 0 ] || return 1
    echo "every run on $sha has finished without it"
}

# Watch one named check on a pull request until it settles, and report what it settled
# as. The pull request is only where the check is read from: it may be a draft, it may
# be waiting on a decision nobody has made, and neither ends this watch.
#
# What is read is `bucket`, not the check's own state. `gh` normalises every forge's
# spelling into pass / fail / pending / skipping / cancel, so there is no in-progress
# word left that could be mistaken for a conclusion — the mistake this mode exists to
# make unavailable.
#
# `skipping` is green here, for the reason `watch_run` gives skipped jobs: a path
# filter deciding a check has nothing to do is the normal shape of a run in this
# repository. `cancel` is red, for the reason `watch_pr` gives it: a cancelled check is
# not going to arrive later.
#
# Naming a check that matches more than one is refused rather than guessed, as an
# ambiguous run is. A verdict from the wrong check is worse than no verdict.
#
# A name that has not appeared yet is the hard case, because slowness and absence look
# alike from here and only one of them is worth waiting out. What tells them apart is
# below: the runs on the pull request's head commit, which is where a check comes from.
watch_check() {
    local pr="$1" name="$2" tries=0 prev="" checks rows count bucket verdict
    echo "watching check \"$name\" on pull request $pr  https://github.com/$repo/pull/$pr"
    while :; do
        # `gh pr checks` reports a red check by exiting non-zero, and refuses outright
        # in the window before the head commit carries any check at all. So its exit is
        # no signal, and its output is read for what it is: a JSON array is an answer,
        # anything else is nothing to go on this round.
        checks=$(gh pr checks "$pr" -R "$repo" --json name,bucket 2>&1) || :
        if rows=$(jq -ec --arg n "$name" 'map(select(.name == $n))' <<< "$checks" 2>/dev/null); then
            count=$(jq 'length' <<< "$rows")
            if [ "$count" -gt 1 ]; then
                echo "✗ \"$name\" names $count checks on pull request $pr — they cannot be told apart:" >&2
                jq -r '.[] | "    \(.name)  \(.bucket)"' <<< "$rows" >&2
                exit 1
            fi
            if [ "$count" = 1 ]; then
                bucket=$(jq -r '.[0].bucket' <<< "$rows")
                if [ "$bucket" != pending ]; then
                    echo "check \"$name\" on pull request $pr: $bucket"
                    # A re-run settles the check again under the same name, which this
                    # watch has already left. Start another one, as `run` asks.
                    case "$bucket" in pass | skipping) return 0 ;; *) return 1 ;; esac
                fi
                tries=0
                sleep "$POLL_SECONDS"
                continue
            fi
        else
            [ "$checks" != "$prev" ] && echo "checks not read: ${checks:-no output}" >&2
            prev=$checks
        fi
        # Not there yet — either the checks could not be read, or none of them carries
        # this name. Whether that is slowness or absence is not for a clock to say, so
        # the runs on the head commit are asked instead: while one of them is going the
        # check may yet be created, and `ci / all green` is not a check at all until
        # every job it waits on has finished. Once they have all finished without it,
        # nothing is going to create it, and that is what ends the wait.
        tries=$((tries + 1))
        if verdict=$(head_runs_settled "$pr"); then
            die "no check named \"$name\" on pull request $pr: $verdict"
        fi
        appear_wait "$tries" "check \"$name\" to appear on pull request $pr"
    done
}

case "$mode" in
    pr)
        [ $# -eq 1 ] || die "usage: watch-ci.sh pr <number>"
        # A pull request is not a run, so there is no id to hand back.
        [ -z "$print_id" ] || die "--print-id takes a run, not a pull request"
        watch_pr "$1"
        ;;
    check)
        [ $# -eq 2 ] || die "usage: watch-ci.sh check <pr> <name>"
        # A check is not a run either, and the run behind it is not what was asked for.
        [ -z "$print_id" ] || die "--print-id takes a run, not a check"
        watch_check "$1" "$2"
        ;;
    main)
        [ $# -eq 1 ] || die "usage: watch-ci.sh main <sha>"
        # Assigned, then watched. resolve_one runs in a command substitution, so its
        # own exit ends only that subshell — inlined as an argument, a refusal to guess
        # would be passed on as if it were an id.
        sha=$(remote_ref_sha "$1") ||
            die "main $1: no such commit on $repo — nothing there will start a run"
        id=$(resolve_one "main $1" "$sha" ci-change.yml)
        deliver "$id"
        ;;
    tag)
        [ $# -eq 1 ] || die "usage: watch-ci.sh tag <tag>"
        # The tag is what starts the run, so the remote's copy of it is the one the run
        # belongs to. Reading it there rather than here also settles the two cases a
        # local read cannot see: a tag never pushed, and one moved since it was.
        sha=$(remote_ref_sha "$1") ||
            die "tag $1 is not on $repo — push it, or nothing will start a run"
        id=$(resolve_one "tag $1" "$sha" release-tag.yml)
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
