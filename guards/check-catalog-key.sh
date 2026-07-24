#!/usr/bin/env bash
# check-catalog-key.sh — fail when the catalog public key amenbo ships and the one the catalog
# repository publishes are not the same key.
#
# Plugin assets are signed by one key held by the catalog CI, and every amenbo verifies against the
# public half compiled into it (`CATALOG_PUBLIC_KEY`). The catalog repository publishes that same
# public half as `catalog-key.pub`, for anyone who wants to check a signature themselves.
#
# Two copies of one value, in two repositories, and only one side is watched: the catalog CI signs a
# probe and verifies it against its own `catalog-key.pub`, which proves the signing key matches the
# published key — and says nothing about the value compiled into amenbo. If those drift, every
# install fails closed with a signature that cannot be verified, and the user sees no cause at all.
#
# Detection is one string comparison, so it is done here rather than left to the symptom.
#
# It belongs with the gates that go red on a tree nobody touched: the other side of the comparison
# lives in another repository and can move with no diff here at all.
#
# Usage: guards/check-catalog-key.sh          (needs network; run by the rot gate)
#        CATALOG_KEY_URL=<url> guards/check-catalog-key.sh   (point it at another copy)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
source_file=$root/crates/amenbo-core/src/plugin_provenance.rs
url=${CATALOG_KEY_URL:-https://raw.githubusercontent.com/ShiroDoromoto/amenbo-plugins/main/catalog-key.pub}

# The compiled-in value, read out of its one declaration.
embedded=$(grep -oE 'CATALOG_PUBLIC_KEY: &str = "[^"]+"' "$source_file" | grep -oE '"[^"]+"' | tr -d '"')
if [ -z "$embedded" ]; then
    echo "✗ no CATALOG_PUBLIC_KEY found in $source_file"
    exit 1
fi

# A minisign public key file is a comment line and then the key itself, so the key is the first line
# that is neither a comment nor empty.
published=$(curl -fsSL "$url" | grep -vE '^\s*(untrusted comment:|#|$)' | head -1 | tr -d '[:space:]')
if [ -z "$published" ]; then
    echo "✗ no key line in $url"
    exit 1
fi

if [ "$embedded" != "$published" ]; then
    echo "✗ the catalog public key has drifted — every plugin install would fail closed"
    echo "  amenbo ships : $embedded"
    echo "    ($source_file)"
    echo "  published    : $published"
    echo "    ($url)"
    echo "  One of the two was rotated without the other. Settle which key is current before shipping."
    exit 1
fi

echo "✓ catalog public key: amenbo and the catalog repository agree ($embedded)"
