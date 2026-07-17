#!/usr/bin/env bash
# check-audit-ignores.sh — fail when an ignored RustSec advisory no longer applies.
#
# `.cargo/audit.toml` ignores advisories we cannot patch: the fix exists upstream but the chain that
# pulls the crate in is semver-pinned, and there is no reachable attack surface here. Each ignore is
# meant to be temporary — it comes off the moment upstream bumps its pin.
#
# Nothing else watches for that moment. `cargo audit` is green whether an ignore still matches
# something or matches nothing at all, so an ignore that has outlived its reason stays green forever,
# and the tree quietly keeps suppressing an advisory it no longer has. That is the leftover-suppression
# failure mode, and the only way to see it is to ask the question directly: does each ignored advisory
# still fire on this lockfile?
#
# So audit each lockfile with the ignores OFF (cargo-audit reads `.cargo/audit.toml` relative to the
# working directory, so running from a directory that has none is what turns them off), collect every
# advisory the tree actually carries, and refuse any ignore that is not among them.
#
# Usage: guards/check-audit-ignores.sh          (needs cargo-audit; run by the weekly rot gate)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
config=$root/.cargo/audit.toml

# The Tauri host crate is excluded from the workspace, so the root audit never sees its lockfile.
locks=("$root/Cargo.lock" "$root/app/src-tauri/Cargo.lock")

# Only the ids inside the `ignore = [...]` list — the justification prose above it names the same ids,
# and grepping the whole file would read those back as if they were entries.
ignored=$(grep -oE '^[[:space:]]*"RUSTSEC-[0-9]{4}-[0-9]{4}"' "$config" | grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' | sort -u)
if [ -z "$ignored" ]; then
    echo "✓ audit ignores: none to check"
    exit 0
fi

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

# Every advisory id the lockfiles carry, ignores disabled. `cargo audit` exits non-zero when it finds
# a vulnerability — which is the normal case here — so the exit code is not the signal; the JSON is.
found=""
for lock in "${locks[@]}"; do
    report=$scratch/$(basename "$(dirname "$lock")").json
    (cd "$scratch" && cargo audit --json --file "$lock" >"$report") || true
    ids=$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    report = json.load(f)
ids = [v["advisory"]["id"] for v in report["vulnerabilities"]["list"]]
for kind in report.get("warnings", {}).values():
    ids += [w["advisory"]["id"] for w in kind if w.get("advisory")]
print("\n".join(ids))
' "$report")
    found="$found$ids
"
done

stale=$(comm -23 <(echo "$ignored") <(echo "$found" | sort -u))
if [ -n "$stale" ]; then
    echo "✗ audit ignores: these no longer apply to any lockfile — drop them from .cargo/audit.toml:"
    while IFS= read -r id; do echo "    $id"; done <<<"$stale"
    echo "  (an ignore that matches nothing suppresses nothing; leaving it there hides the next advisory"
    echo "   that happens to reuse the id's justification, and it is how a temporary waiver becomes permanent)"
    exit 1
fi

echo "✓ audit ignores: all still apply"
