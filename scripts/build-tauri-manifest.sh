#!/usr/bin/env bash
# Assemble the Tauri updater manifest (latest-tauri.json) the GUI self-update path reads.
#
# It is a SEPARATE file from wharfy's latest.json: wharfy's is amenbo's own "is there a newer
# version" check (its own schema), this one is tauri-plugin-updater's fixed schema
# (version + platforms{os-arch: {signature, url}}). Two files, two consumers, one release job = same
# version, so they never skew. wharfy stays a generic distributor and does not carry this.
#
# It reads each per-platform updater artifact's detached minisign signature (<artifact>.sig, written
# by `tauri signer sign` / createUpdaterArtifacts) out of dist/ and points url at that artifact's
# release-download URL. A platform whose .sig is absent is skipped, so the set can grow (Linux
# AppImage arrives with the AppImage self-update work) without touching this script.
#
# Usage: build-tauri-manifest.sh <tag> [dist_dir]   (tag e.g. v1.6.0)
set -euo pipefail

tag="${1:?usage: build-tauri-manifest.sh <tag> [dist_dir]}"
dist="${2:-dist}"
version="${tag#v}"
base="https://github.com/ShiroDoromoto/amenbo/releases/download/${tag}"
pub_date="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# platform-key : dist artifact basename. The url is base/<artifact>; the signature is <artifact>.sig.
# Linux's updater artifact is the AppImage itself (Tauri v2 re-uses it — no .tar.gz); its .sig rides
# beside it. A row whose .sig is absent is skipped, so a keyless build simply drops that platform.
#
# Every row names an asset carrying `-update`, on every platform. GitHub reports one download count per
# asset, so an asset serving both audiences reports a sum that cannot be split back into its parts —
# and reading which assets are the update ones is then one rule about the name rather than one rule per
# suffix. Windows and Linux reach it by copying the installer under that name (same bytes, same
# signature; the release job makes the copies before it calls this script); macOS has no copy to make,
# its updater archive being a separate artifact already, so it simply carries the name.
rows=(
  "darwin-aarch64:amenbo-darwin-arm64-update.app.tar.gz"
  "darwin-x86_64:amenbo-darwin-amd64-update.app.tar.gz"
  "windows-x86_64:amenbo-app-windows-x64-setup-update.exe"
  "linux-x86_64:amenbo-app-linux-x86_64-update.AppImage"
)

platforms='{}'
for row in "${rows[@]}"; do
  key="${row%%:*}"
  artifact="${row#*:}"
  sig="${dist}/${artifact}.sig"
  if [ ! -f "$sig" ]; then
    echo "build-tauri-manifest: no signature for ${key} (${sig} absent) — skipping" >&2
    continue
  fi
  platforms="$(jq \
    --arg key "$key" \
    --arg sig "$(cat "$sig")" \
    --arg url "${base}/${artifact}" \
    '. + {($key): {signature: $sig, url: $url}}' <<<"$platforms")"
done

if [ "$(jq 'length' <<<"$platforms")" -eq 0 ]; then
  echo "build-tauri-manifest: no signed updater artifacts found in ${dist}/" >&2
  exit 1
fi

jq -n \
  --arg version "$version" \
  --arg pub_date "$pub_date" \
  --argjson platforms "$platforms" \
  '{version: $version, pub_date: $pub_date, platforms: $platforms}' \
  >"${dist}/latest-tauri.json"

echo "→ ${dist}/latest-tauri.json (version ${version}, platforms: $(jq -r '.platforms | keys | join(", ")' "${dist}/latest-tauri.json"))"
