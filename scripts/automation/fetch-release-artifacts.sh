#!/usr/bin/env bash
# fetch-release-artifacts.sh — a script step: download what a release run built, and see that each
# file is the same bytes as the asset of the same name on the release.
#
# The artifacts are taken from the Actions run, never from the release's download URLs. Every artifact
# of the run whose name starts with `dist-` is downloaded into a folder of its own under the place:
# `$TMPDIR/AMB-T-<task>/release-<version>/`, or `$TMPDIR/amenbo-release-<version>/` when no task is
# given. A folder an earlier download left there is replaced. The release's digests are written to
# `release-digests.txt` in the place, one `<digest> <name>` per line, and each downloaded file's
# `sha256:` is compared with the line of the same name, looked up by exact match:
#
#   same          OK
#   no such line  a file the release does not carry, which is normal
#   different     red
#
# The step names its ways out and ports in Japanese, so the names below are the ones written in the
# code:
#
#   done   every file the release also carries is the same. Hands on the place's path
#   red    a file differs. Hands on, as a file beside output.json, the files that differ and the place
#
# Inputs, from AMENBO_INPUT's `ins`: the version, without the leading `v` (required); the run's id
# (required); the task (optional), by its ref or its number.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# do — no version, no run, a run with no `dist-` artifact, a release that does not exist, a download
# that failed — ends it by its error way out, with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

PLACE_OUTPUT="置き場"
RED_FILE="成果物を落として照合する.md"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

version=$(input_version)
run=$(input run)
[[ "$run" =~ ^[0-9]+$ ]] || die "入力に run の id がありません：\"$run\""
task=$(input タスク)
task="${task#AMB-T-}"
[ -z "$task" ] || [[ "$task" =~ ^[0-9]+$ ]] || die "入力のタスクがタスクの番号ではありません：\"$task\""
resolve_repo

tmp="${TMPDIR:-/tmp}"
tmp="${tmp%/}"
if [ -n "$task" ]; then
    place="$tmp/AMB-T-$task/release-$version"
else
    place="$tmp/amenbo-release-$version"
fi
mkdir -p "$place" || die "$place を作れませんでした"

names=$(gh api "repos/$repo/actions/runs/$run/artifacts" --paginate --jq '.artifacts[].name | select(startswith("dist-"))') ||
    die "run の成果物の一覧を読めませんでした：$(run_url "$run")"
[ -n "$names" ] || die "run に dist-* という名前の成果物がありません：$(run_url "$run")"

while IFS= read -r name; do
    rm -rf "${place:?}/$name"
    gh run download "$run" --repo "$repo" --name "$name" -D "$place/$name" >&2 ||
        die "$name をダウンロードできませんでした：$(run_url "$run")"
done <<< "$names"

gh api "repos/$repo/releases/tags/v$version" --jq '.assets[] | "\(.digest) \(.name)"' > "$place/release-digests.txt" ||
    die "$repo のリリース v$version を読めませんでした"

ok=0
absent=0
differ=()
while IFS= read -r -d '' file; do
    base=$(basename "$file")
    want=$(awk -v n="$base" '$2 == n { print $1; exit }' "$place/release-digests.txt")
    if [ -z "$want" ]; then
        absent=$((absent + 1))
        continue
    fi
    got="sha256:$(shasum -a 256 "$file" | awk '{ print $1 }')"
    if [ "$got" = "$want" ]; then
        ok=$((ok + 1))
    else
        differ+=("${file#"$place"/}: downloaded $got, release $want")
    fi
done < <(find "$place" -mindepth 2 -type f -path "$place/dist-*" -print0)

counts="一致 ${ok}件、リリースに無い ${absent}件、違う ${#differ[@]}件"
if [ "${#differ[@]}" = 0 ]; then
    leave 完了 "$(jq -n --arg k "$PLACE_OUTPUT" --arg p "$place" '{($k): $p}')" \
        "run の成果物はリリース v$version と一致しました（${counts}）。ダウンロード先：${place}。run：$(run_url "$run")"
fi

{
    echo "# The artifacts differ from the release v$version"
    echo
    echo "- run: $(run_url "$run")"
    echo "- place: $place"
    echo "- digests of the release: $place/release-digests.txt"
    echo "- $counts"
    echo
    printf -- '- %s\n' "${differ[@]}"
} > "$out_dir/$RED_FILE"
leave_red "$RED_FILE" "run の成果物がリリース v$version と違います（${counts}）。run：$(run_url "$run")"
