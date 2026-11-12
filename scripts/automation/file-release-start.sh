#!/usr/bin/env bash
# file-release-start.sh — a script step: once the Viewer app has gone to both stores, file the task of
# starting its release when the review passes, for the human, and hold it as waiting on the review.
#
# It takes no arguments. Its inputs are the version (VERSION_INPUT below), the build number
# (BUILD_INPUT), a sentence on where each store stands (STATE_INPUT), and, which may be left unwired, the
# folder the task is worked in (FOLDER_INPUT) and the run's task (TASK_INPUT). The run's task, where it is
# not wired, is the task the run took last, read from the run that started this step (AMENBO_RUN); the
# folder, where it is not wired, is the run's task's own.
#
# It files the task titled from the version, for the human, classified with the product, the main theme
# and the release kind of work — with no value of the release axis and under no role — its notes giving
# the version, the build number, where the stores stand and the commands that start the release. It
# finishes creating it, marks it blocked as waiting on the stores' review, and adds a comment to the run's
# task naming the version, where the stores stand and the task it filed. It has one way out, done, and
# its report names the task it filed.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — a missing input, no task to write on, a command that failed — ends it by its error way out,
# with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# project the task is filed in.
set -euo pipefail

# The names the board writes in Japanese: the inputs, the product and the way out.
VERSION_INPUT="版"
BUILD_INPUT="ビルド番号"
STATE_INPUT="ストアの状態"
FOLDER_INPUT="作業フォルダ"
TASK_INPUT="タスク"
PRODUCT="Viewerアプリ"
BLOCK_REASON="ストアの審査待ち"
DONE_EXIT="完了"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

input() { jq -r --arg i "$1" '.ins[$i] // empty' "$AMENBO_INPUT" || die "$AMENBO_INPUT を読めませんでした"; }
version=$(input "$VERSION_INPUT")
[ -n "$version" ] || die "バージョンがありません：バージョンを、このステップの「${VERSION_INPUT}」につないでください"
build=$(input "$BUILD_INPUT")
[ -n "$build" ] || die "ビルド番号がありません：ビルド番号を、このステップの「${BUILD_INPUT}」につないでください"
state=$(input "$STATE_INPUT")
[ -n "$state" ] || die "ストアの状態がありません：ストアの状態を、このステップの「${STATE_INPUT}」につないでください"
folder=$(input "$FOLDER_INPUT")
task=$(input "$TASK_INPUT")

amenbo_ai() { amenbo "$@" --actor ai; }

if [ -z "$task" ]; then
    [ -n "${AMENBO_RUN:-}" ] || die "タスクがありません：「${TASK_INPUT}」にタスクをつなぐか、run のステップとして動かしてください"
    id=$(amenbo_ai automation run-show "$AMENBO_RUN" --json | jq -r '.tasks[-1].task_id // empty') ||
        die "run $AMENBO_RUN を読めませんでした"
    [ -n "$id" ] || die "run $AMENBO_RUN はタスクを取っていません"
    task="AMB-T-$id"
fi

if [ -z "$folder" ]; then
    folder=$(amenbo_ai task show "$task" --json | jq -r '.at.dir // empty') || die "$task を読めませんでした"
fi
at_args=()
[ -z "$folder" ] || at_args=(--at "$folder")

# shellcheck disable=SC2016 # the backticks are Markdown, not a command
notes=$(printf '%s\n' \
    "- バージョン：$version" \
    "- ビルド番号：$build" \
    "- ストアの状態：$state" \
    '' \
    '## 配信開始の手順' \
    "- App Store：\`uv run app/tool/release/appstore.py release $version\`" \
    '- Google Play：`uv run app/tool/release/play.py promote --from alpha --to production` のあと、Play の審査を経て配信される')

title="${version}（${PRODUCT}）の審査が通ったら配信を始める"
id=$(amenbo_ai task add --title "$title" --notes - --to human ${at_args[@]+"${at_args[@]}"} \
    --dim "プロダクト=$PRODUCT" --dim テーマ=メイン --dim 作業種別=リリース作業 \
    --json <<< "$notes" | jq -r '.task.id // empty') || die "配信開始のタスクを起票できませんでした"
[ -n "$id" ] || die "配信開始のタスクを起票しましたが、タスクの id を読めませんでした"
filed="AMB-T-$id"
amenbo_ai task finish-creating "$filed" >&2 || die "$filed の作成を終えられませんでした"
amenbo_ai task block "$filed" --reason "$BLOCK_REASON" >&2 || die "$filed を blocked にできませんでした"

printf '%s\n' "バージョン ${version}（ビルド ${build}）を両ストアへ出しました。" "- ストアの状態：$state" \
    "- 配信開始のタスク：$filed" | amenbo_ai comment add "$task" --text - >&2 ||
    die "$task にコメントを付けられませんでした"

report="配信開始のタスク ${filed}「${title}」を起票し、ストアの審査待ちにしました。${task} にコメントしました。"
jq -n --arg exit "$DONE_EXIT" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {}, report: $report}' > "$AMENBO_OUTPUT"
