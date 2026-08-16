#!/usr/bin/env bash
# go-gate.sh — the devtool module's gate: formatting, vet, and its tests.
#
# devtool is the optional parallel-development helper (devtool/README.md). Nothing else in this
# tree compiles it, so nothing else would notice it breaking — this is the only thing that does.
#
# It lives in a script rather than in the Makefile recipe and the CI job so that the three checks
# are declared once: `make go-gate` and _ci.yml's `go` job both run this file, and a verdict in one
# place is the verdict in the other. Being a tracked .sh, it is itself covered by shell-gate.
#
# gofmt is the whole formatting argument in Go — there is nothing to configure and no style to
# agree on — but `gofmt -l` reports by printing names and still exits 0, so the refusal has to be
# spelled out here.
#
# Exit codes: 0 = clean, 1 = a check failed.

set -euo pipefail

cd "$(dirname "$0")/../devtool"

unformatted="$(gofmt -l .)"
if [ -n "$unformatted" ]; then
  echo "go-gate.sh: gofmt would rewrite these files — run gofmt -w on them:" >&2
  echo "$unformatted" >&2
  exit 1
fi

go vet ./...
go test ./...
