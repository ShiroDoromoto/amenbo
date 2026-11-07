#!/usr/bin/env node
// check-third-party-notices.mjs — say when THIRD-PARTY-NOTICES no longer matches what the lockfiles
// ship.
//
// The notices file is committed rather than written at release time, so a release can carry it as a
// plain file beside LICENSE and a reader can open it to see what amenbo carries. That only
// holds while it is what scripts/gen-third-party-notices.mjs would write for the current
// Cargo.lock and app/src-tauri/Cargo.lock — and Dependabot bumps are auto-merged, so nobody would
// regenerate it by hand.
//
// Two questions, both a red:
//   1. Do the bundled C sources still match BUNDLED_SOURCES? libsqlite3-sys and openssl-src vendor
//      SQLCipher, SQLite and OpenSSL, and no crate declares those licenses — so a bump of either
//      carrier, or a new `*-src` crate, has to be read by a human before it ships.
//   2. Is the committed file what the generator writes for this tree?
//
// Usage: node guards/check-third-party-notices.mjs   (from anywhere; needs cargo, fetches crates)

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import {
  NoticeError,
  NOTICES,
  checkBundledSources,
  collectShipped,
  generate,
} from '../scripts/gen-third-party-notices.mjs'

// Pure: the committed text and the regenerated one in, the first lines that differ out (or null).
export function firstDifference(committed, fresh) {
  if (committed === fresh) return null
  const a = committed.split('\n')
  const b = fresh.split('\n')
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if (a[i] !== b[i]) return { line: i + 1, committed: a[i] ?? '(end of file)', fresh: b[i] ?? '(end of file)' }
  }
  return null
}

function main() {
  let committed
  try {
    committed = readFileSync(NOTICES, 'utf8')
  } catch {
    console.error('✗ THIRD-PARTY-NOTICES is missing — run node scripts/gen-third-party-notices.mjs')
    return 1
  }

  const packages = collectShipped()
  const problems = checkBundledSources(packages)
  if (problems.length > 0) {
    console.error('✗ the bundled sources no longer match BUNDLED_SOURCES (scripts/gen-third-party-notices.mjs):')
    for (const p of problems) console.error(`    ${p}`)
    return 1
  }

  const diff = firstDifference(committed, generate(packages))
  if (diff) {
    console.error('✗ THIRD-PARTY-NOTICES is not what the lockfiles generate. First difference, line ' + diff.line + ':')
    console.error(`    committed: ${diff.committed}`)
    console.error(`    generated: ${diff.fresh}`)
    console.error('  Run node scripts/gen-third-party-notices.mjs and commit the result.')
    return 1
  }

  console.log(`✓ THIRD-PARTY-NOTICES matches the ${packages.length} shipped crates in the lockfiles`)
  return 0
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    process.exit(main())
  } catch (e) {
    if (!(e instanceof NoticeError)) throw e
    console.error(`✗ ${e.message}`)
    process.exit(1)
  }
}
