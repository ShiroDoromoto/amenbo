#!/usr/bin/env bash
# open-pr.sh — a script step: open the pull request of a task's branch with auto-merge on, or find the
# one already open, and hand its number on.
#
# It works in the task's worktree, on the branch checked out there, and does not merge main into it.
# What it does depends on the pull request the branch already has:
#
#   open                 pushes the branch (with --force-with-lease, as an amended commit is a
#                        rewrite) and leaves auto-merge as it was set
#   merged, nothing new  does nothing
#   none, or merged or closed with commits since
#                        folds the branch's commits into one when it carries two or more, merge
#                        commits not counted, reusing the message of the oldest; pushes the branch;
#                        opens the pull request from that commit against main; and turns auto-merge
#                        on with a merge commit
#
# The commits are folded onto the point the branch left main at (`git merge-base`), not onto
# origin/main, so a branch behind main does not take main's newer changes back out.
#
# The step names its ways out and ports in Japanese, so the names below are the ones written in the
# code. It has one way out, done, which hands on the pull request's number. Input, from AMENBO_INPUT's
# `ins`: the worktree's folder (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# do — no worktree, a worktree on main or on no branch, a branch with nothing to open, a command that
# failed — ends it with exit code 1, which the step leaves through its error way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in to read
# its helpers, then works in the worktree, which is where `git` and `gh` read the repository from.
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

worktree=$(input worktree)
[ -n "$worktree" ] && [ -d "$worktree" ] || die "no worktree folder in the input: \"$worktree\""
cd "$worktree"

branch=$(git branch --show-current)
[ -n "$branch" ] || die "the worktree $worktree is on no branch"
[ "$branch" != main ] || die "the worktree $worktree is on main, and main is not opened as a pull request"

git fetch origin -q || die "could not fetch origin"

opened() { leave 完了 "$(jq -n --arg n "$1" '{"PR": $n}')" "$2"; }

# `gh pr view` fails when the branch has no pull request, which reads here as none.
pr_json=$(gh pr view "$branch" --json number,state,url 2>/dev/null) || pr_json='{}'
pr=$(jq -r '.number // empty' <<< "$pr_json")
state=$(jq -r '.state // empty' <<< "$pr_json")
ahead=$(git rev-list --count origin/main..HEAD)

if [ "$state" = MERGED ] && [ "$ahead" = 0 ]; then
    opened "$pr" "pull request $pr of $branch is already merged, and the branch carries nothing since: $(jq -r .url <<< "$pr_json")"
fi

if [ "$state" = OPEN ]; then
    git push -q --force-with-lease -u origin "$branch" || die "could not push $branch"
    opened "$pr" "pull request $pr of $branch is open; pushed the branch to it: $(jq -r .url <<< "$pr_json")"
fi

commits=$(git rev-list --no-merges --count origin/main..HEAD)
[ "$commits" -gt 0 ] || die "$branch carries no commit that main does not, so there is nothing to open"

if [ "$commits" -gt 1 ]; then
    oldest=$(git rev-list --no-merges --reverse origin/main..HEAD | head -n 1)
    base=$(git merge-base origin/main HEAD) || die "$branch shares no history with origin/main"
    git reset -q --soft "$base" || die "could not fold the commits of $branch"
    git commit -q -C "$oldest" || die "could not commit the folded commits of $branch"
    echo "folded $commits commits of $branch into $(git rev-parse --short HEAD)" >&2
fi

git push -q --force-with-lease -u origin "$branch" || die "could not push $branch"
url=$(gh pr create --fill --base main --head "$branch") || die "could not open the pull request of $branch"
pr=$(sed -nE 's|.*/pull/([0-9]+)$|\1|p' <<< "$url" | tail -n 1)
[ -n "$pr" ] || die "opening the pull request of $branch gave no number: $url"
gh pr merge "$pr" --merge --auto || die "opened pull request $pr, and could not turn auto-merge on: $url"

opened "$pr" "opened pull request $pr of $branch with auto-merge on: $url"
