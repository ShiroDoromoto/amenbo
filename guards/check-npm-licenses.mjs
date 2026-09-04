#!/usr/bin/env node
// check-npm-licenses.mjs — the npm half of the license gate.
//
// `cargo deny check licenses` judges what the Rust side ships. It never sees the GUI's npm
// dependencies, yet those ride into the Tauri bundle just the same, so a strong-copyleft package
// arriving there would make what we distribute undistributable under Apache-2.0. Nobody reads a
// license at bump time — Dependabot bumps are auto-merged — so this gate is the only thing that
// would notice one.
//
// The allow-list is NOT repeated here: it is read out of deny.toml, so the two ecosystems cannot
// drift into disagreeing about what amenbo may ship. What amenbo is allowed to ship is one policy,
// and it is written down once.
//
// Scope is the shipped half of the tree: the dev toolchain (vite/vitest/typescript) does not ride
// into the bundle, so its licenses do not bind what we distribute. Same split as the npm audit gate.
//
// Scope is also one license per package, which is as deep as npm records. A package that is a
// container for other people's work — `@shikijs/langs` ships 361 TextMate grammars collected from
// as many projects — declares its own license and says nothing about theirs. That question is
// check-grammar-licenses.mjs's, and it reads the same allow-list this file parses.
//
// The source of truth is package-lock.json, not an installed node_modules. npm records each
// resolved package's license in the lockfile, so the verdict is a pure function of the file CI's
// path filter already watches — no install step, nothing to drift, and the mirror of the Rust gate
// reading Cargo.lock. (It also means the gate cannot be fooled by an edited node_modules: `npm ls`
// answers from the lockfile anyway.)
//
// Usage: node guards/check-npm-licenses.mjs   (from the repo root)

import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const LOCKFILE = join(ROOT, 'app', 'package-lock.json')

// A package that declares no `license` field at all. npm cannot answer for it, so a human read the
// bundled license text once and wrote the verdict down here — with the evidence, so the next reader
// can check it rather than trust it. Keep this list at zero entries if you can.
//
// An entry is a claim that keeps being tested: `licenseFile` must still say what it says, and the
// package must still declare nothing. The moment upstream adds a license field, the entry is a
// waiver suppressing a question that answers itself — so the gate says to drop it (the same reason
// guards/check-audit-ignores.sh exists: an ignore nobody revisits is an ignore nobody notices).
const NO_LICENSE_FIELD = {
  khroma: {
    license: 'MIT',
    licenseFile: 'license',
    why: 'Ships the MIT text verbatim ("The MIT License (MIT)", with its copyright line) in the file named below, but declares no license field. A transitive dep of mermaid.',
  },
}

// A gate that cannot say why it stopped is a gate nobody fixes. `main` turns this into an exit;
// the pure functions below throw it, so their refusals are observable (and testable) rather than
// killing the process from the inside.
class GateError extends Error {}
const refuse = (msg) => {
  throw new GateError(msg)
}

// --- the allow-list, read from the Rust gate's config ------------------------------------------

// deny.toml is TOML and node has no TOML parser, but the shape we need is one array of quoted
// strings under [licenses]. Anything unexpected must stop the gate rather than silently yield an
// empty (= everything fails) or partial (= something slips) allow-list.
export function parseAllowList(toml) {
  const licenses = toml.split(/^\[licenses\]$/m)[1]
  if (licenses === undefined) refuse('deny.toml has no [licenses] section — the allow-list moved; fix this gate')

  const section = licenses.split(/^\[/m)[0] // stop at the next table header
  const allow = section.match(/allow\s*=\s*\[([^\]]*)\]/)
  if (!allow) refuse('deny.toml [licenses] has no allow = [...] — the allow-list moved; fix this gate')

  // Drop commented-out entries: deny.toml annotates the list, and a `# "GPL-3.0",` line explaining
  // what we do NOT allow must not be read as allowing it.
  const body = allow[1].replace(/#[^\n]*/g, '')
  const ids = [...body.matchAll(/"([^"]+)"/g)].map((m) => m[1])
  if (ids.length === 0) refuse('deny.toml allow-list parsed as empty — refusing to judge')
  return new Set(ids)
}

const readAllowList = () => parseAllowList(readFileSync(join(ROOT, 'deny.toml'), 'utf8'))

// --- SPDX expressions --------------------------------------------------------------------------

// npm licenses are SPDX expressions, not bare ids: "MIT OR Apache-2.0" is a choice (allowed if
// EITHER side is), "MIT AND CC0-1.0" is a conjunction (allowed only if BOTH are). Treating the
// string as an opaque id would fail every dual-licensed package, and matching a substring would
// pass "MIT AND GPL-3.0". So parse it.
export function tokenize(expr) {
  return expr
    .replace(/[()]/g, (p) => ` ${p} `)
    .split(/\s+/)
    .filter(Boolean)
}

// expr := term (OR term)* · term := factor (AND factor)* · factor := "(" expr ")" | id
// `WITH` binds to the id before it ("Apache-2.0 WITH LLVM-exception" is one identifier).
export function parse(tokens) {
  let i = 0
  const peek = () => tokens[i]
  const next = () => tokens[i++]

  const factor = () => {
    if (peek() === '(') {
      next()
      const inner = expr()
      if (next() !== ')') throw new Error('unbalanced parentheses')
      return inner
    }
    let id = next()
    if (id === undefined) throw new Error('unexpected end of expression')
    while (peek() === 'WITH') {
      next()
      const exception = next()
      if (exception === undefined) throw new Error('WITH without an exception')
      id = `${id} WITH ${exception}`
    }
    return { kind: 'id', id }
  }

  const term = () => {
    let node = factor()
    while (peek() === 'AND') {
      next()
      node = { kind: 'and', left: node, right: factor() }
    }
    return node
  }

  const expr = () => {
    let node = term()
    while (peek() === 'OR') {
      next()
      node = { kind: 'or', left: node, right: term() }
    }
    return node
  }

  const node = expr()
  if (i !== tokens.length) throw new Error(`trailing tokens: ${tokens.slice(i).join(' ')}`)
  return node
}

export function allowed(node, allow) {
  switch (node.kind) {
    case 'id':
      return allow.has(node.id.replace(/\+$/, '')) // "Apache-2.0+" = that version or later
    case 'or':
      return allowed(node.left, allow) || allowed(node.right, allow)
    case 'and':
      return allowed(node.left, allow) && allowed(node.right, allow)
  }
}

// --- the shipped tree --------------------------------------------------------------------------

export function shippedPackages(lock) {
  if (!(lock.lockfileVersion >= 3)) {
    // v1 lockfiles record no licenses at all, so a gate reading one would pass everything.
    refuse(`app/package-lock.json is lockfileVersion ${lock.lockfileVersion}; this gate needs 3+ (npm 7+)`)
  }

  const found = new Map() // "node_modules/..." path -> {name, version, license}
  for (const [path, entry] of Object.entries(lock.packages ?? {})) {
    if (path === '') continue // the root package: amenbo's own GUI, licensed by us
    // `dev` = reachable only through devDependencies. `devOptional` = only through dev and/or
    // optional edges. Neither ships, and excluding both is exactly what `npm ls --omit=dev` lists.
    // A plain `optional` prod dependency is NOT excluded: it can ship, so it is judged.
    if (entry.dev || entry.devOptional) continue
    if (entry.link) continue // a workspace symlink, judged where it really lives
    found.set(path, {
      name: path.replace(/^.*node_modules\//, ''),
      version: entry.version,
      license: entry.license,
    })
  }

  if (found.size === 0) refuse('app/package-lock.json lists no shipped dependencies — refusing to pass vacuously')
  return found
}

// --- the judgment ------------------------------------------------------------------------------

// Pure: packages + allow-list + exceptions in, verdict out. Nothing read, nothing printed, nothing
// exited — which is what lets the tests drive it with a synthetic tree and assert that the things
// that MUST go red actually do. A gate is only worth its runtime if its "no" can be demonstrated.
export function judge(packages, allow, exceptions = NO_LICENSE_FIELD) {
  const violations = []
  const undeclared = []
  const usedExceptions = new Set()

  for (const { name, version, license } of packages.values()) {
    if (!license) {
      const known = exceptions[name]
      if (!known) {
        undeclared.push(`${name}@${version} declares no license`)
        continue
      }
      usedExceptions.add(name)
      if (!allowed({ kind: 'id', id: known.license }, allow)) {
        violations.push(`${name}@${version}: ${known.license} (read from its ${known.licenseFile}) is not allowed`)
      }
      continue
    }

    let ok
    try {
      ok = allowed(parse(tokenize(license)), allow)
    } catch (e) {
      violations.push(`${name}@${version}: cannot read the license expression "${license}" (${e.message})`)
      continue
    }
    if (!ok) violations.push(`${name}@${version}: ${license} is not allowed`)
  }

  // The other half of an exception: one that no longer applies is green either way, so nothing would
  // ever notice it went stale. Ask the question the check above does not.
  for (const name of Object.keys(exceptions)) {
    if (usedExceptions.has(name)) continue
    const shipped = [...packages.values()].find((p) => p.name === name)
    if (!shipped) {
      violations.push(`${name} is exempted here but is no longer a shipped dependency — drop the entry`)
    } else {
      violations.push(
        `${name}@${shipped.version} now declares "${shipped.license}" itself — drop the entry, it is suppressing a question that answers itself`,
      )
    }
  }

  return { violations, undeclared, usedExceptions }
}

// --- the gate ----------------------------------------------------------------------------------

function main() {
  const allow = readAllowList()
  const packages = shippedPackages(JSON.parse(readFileSync(LOCKFILE, 'utf8')))
  const { violations, undeclared, usedExceptions } = judge(packages, allow)

  if (undeclared.length > 0) {
    console.error('✗ shipped packages declare no license, and this gate will not guess:')
    for (const u of undeclared) console.error(`    ${u}`)
    console.error("  Read the package's bundled license text and either drop the dependency or record")
    console.error('  the verdict (with its evidence) in NO_LICENSE_FIELD in this file.')
  }
  if (violations.length > 0) {
    console.error('✗ license gate:')
    for (const v of violations) console.error(`    ${v}`)
    console.error("  The allow-list is deny.toml's [licenses] allow — amenbo ships Apache-2.0 binaries, so a")
    console.error('  package outside it cannot ride along. Drop the dependency, or take the change to the')
    console.error('  allow-list deliberately (it governs the Rust side too).')
  }
  if (undeclared.length > 0 || violations.length > 0) return 1

  console.log(`→ npm licenses: ${packages.size} shipped packages, all within deny.toml's allow-list`)
  for (const name of usedExceptions) {
    const { license, licenseFile } = NO_LICENSE_FIELD[name]
    console.log(`  (${name}: no license field — ${license} read from its ${licenseFile})`)
  }
  return 0
}

// Only when run as the gate. Imported (by its tests), this file defines and does nothing.
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    process.exit(main())
  } catch (e) {
    if (!(e instanceof GateError)) throw e
    console.error(`✗ ${e.message}`)
    process.exit(1)
  }
}
