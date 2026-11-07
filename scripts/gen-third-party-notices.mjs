#!/usr/bin/env node
// gen-third-party-notices.mjs — write THIRD-PARTY-NOTICES, the license texts of what amenbo ships
// that amenbo did not write.
//
// The binaries statically link every Rust crate they depend on, so each crate's notice has to travel
// with them. `cargo deny check licenses` (deny.toml) judges whether a crate MAY ship; it does not
// carry the notice, and it only reads what a crate declares. That second half matters here: rusqlite
// is built with `bundled-sqlcipher-vendored-openssl`, so SQLCipher (BSD-3-Clause), SQLite (public
// domain) and OpenSSL (Apache-2.0) are compiled into the binaries from C sources vendored inside two
// crates — libsqlite3-sys, which declares only `MIT`, and openssl-src, whose `MIT/Apache-2.0` is the
// crate's build glue, not OpenSSL. No declaration names them, so BUNDLED_SOURCES below names them,
// by crate AND version: a bump of either carrier, or a new `*-src` crate, stops the gate until a
// human has looked at what the new sources say (guards/check-third-party-notices.mjs).
//
// What counts as shipped: the crates reachable from the shipped roots (the `amenbo` CLI, the askpass
// helper, the GUI host crate) over normal and build edges, on every target platform. Dev edges are
// not followed — test-only crates (amenbo-scratch, …) never reach a binary. Build edges ARE followed:
// openssl-src is a build-dependency of openssl-sys, and what it builds is linked in. That also counts
// pure build tools (cc, syn, …), which is a notice too many rather than one too few.
//
// The source of truth is the lockfiles, read through `cargo metadata --locked` (which also fetches the
// crate sources the license texts are read from). Versions are left out of the file on purpose: a
// patch bump that does not change a crate's license text leaves THIRD-PARTY-NOTICES as it was, so an
// auto-merged Dependabot bump only goes red when there is something to re-read.
//
// Usage: node scripts/gen-third-party-notices.mjs   (from anywhere; writes <repo>/THIRD-PARTY-NOTICES)

import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
export const NOTICES = join(ROOT, 'THIRD-PARTY-NOTICES')

// The cargo workspaces whose lockfiles ship, and the crates in each that become a shipped binary.
// verification/ has a lockfile too but is never distributed.
export const WORKSPACES = [
  { dir: '.', lockfile: 'Cargo.lock', roots: ['amenbo-cli', 'amenbo-askpass'] },
  { dir: 'app/src-tauri', lockfile: 'app/src-tauri/Cargo.lock', roots: ['amenbo-app'] },
]

// Third-party sources compiled into the binaries that no crate declares. Keyed by the crate that
// carries them and its exact version: the license of vendored C code is a fact about those sources,
// so it is re-read whenever they change. `files` are paths inside the crate whose text is reproduced;
// `statement` is used where upstream has no license file, with `evidence` pinning the source line
// it was copied from.
export const BUNDLED_SOURCES = [
  {
    crate: 'libsqlite3-sys',
    version: '0.38.2',
    component: 'SQLCipher',
    license: 'BSD-3-Clause',
    files: ['sqlcipher/LICENSE'],
  },
  {
    crate: 'libsqlite3-sys',
    version: '0.38.2',
    component: 'SQLite (inside the SQLCipher amalgamation)',
    license: 'blessing (public domain)',
    evidence: { file: 'sqlcipher/sqlite3.c', contains: 'The author disclaims copyright to this source code.' },
    statement: [
      'The author disclaims copyright to this source code.  In place of',
      'a legal notice, here is a blessing:',
      '',
      '   May you do good and not evil.',
      '   May you find forgiveness for yourself and forgive others.',
      '   May you share freely, never taking more than you give.',
    ].join('\n'),
  },
  {
    crate: 'openssl-src',
    version: '300.6.1+3.6.3',
    component: 'OpenSSL',
    license: 'Apache-2.0',
    files: ['openssl/LICENSE.txt'],
  },
]

// A crate named like this exists to vendor someone else's sources into the build.
export const SOURCE_CARRIER = /-src$/

// The files at a crate's top level that carry its license or notice.
const LICENSE_FILE = /^(licen[cs]e|copying|copyright|notice|unlicense)([-._].*)?$/i

// Pure functions throw this, so their refusals are observable (and testable).
export class NoticeError extends Error {}
const refuse = (msg) => {
  throw new NoticeError(msg)
}

// --- the shipped tree ---------------------------------------------------------------------------

// metadata: the JSON `cargo metadata --format-version 1` prints. Returns the third-party packages
// reachable from `roots` over normal and build edges, keyed by package id.
export function shippedPackages(metadata, roots) {
  const byId = new Map(metadata.packages.map((p) => [p.id, p]))
  const nodes = new Map((metadata.resolve?.nodes ?? []).map((n) => [n.id, n]))
  if (nodes.size === 0) refuse('cargo metadata has no resolve graph — refusing to pass vacuously')

  const queue = []
  for (const name of roots) {
    const root = metadata.packages.find((p) => p.name === name && p.source === null)
    if (!root) refuse(`shipped root ${name} is not in this workspace — the list in WORKSPACES is stale`)
    queue.push(root.id)
  }

  const seen = new Set(queue)
  while (queue.length > 0) {
    const node = nodes.get(queue.shift())
    if (!node) continue
    for (const dep of node.deps ?? []) {
      // dep_kinds: kind null = normal, "build", "dev"; one entry per (kind, target).
      if (!dep.dep_kinds.some((k) => k.kind !== 'dev')) continue
      if (seen.has(dep.pkg)) continue
      seen.add(dep.pkg)
      queue.push(dep.pkg)
    }
  }

  const shipped = new Map()
  for (const id of seen) {
    const pkg = byId.get(id)
    if (pkg && pkg.source !== null) shipped.set(id, pkg) // source null = amenbo's own crates
  }
  if (shipped.size === 0) refuse('no third-party crate is reachable from the shipped roots — refusing to pass vacuously')
  return shipped
}

// --- the bundled C sources ------------------------------------------------------------------------

// packages: every shipped package (all workspaces). Says what does not match BUNDLED_SOURCES; an
// empty list means every carrier is accounted for at the version its entry was read at.
export function checkBundledSources(packages, table = BUNDLED_SOURCES) {
  const problems = []
  const carriers = new Set(table.map((e) => e.crate))
  const shipped = [...packages]

  for (const crate of carriers) {
    const versions = new Set(shipped.filter((p) => p.name === crate).map((p) => p.version))
    const expected = new Set(table.filter((e) => e.crate === crate).map((e) => e.version))
    if (versions.size === 0) {
      problems.push(`${crate} is in BUNDLED_SOURCES but no longer ships — find what carries those sources now and update the entry`)
      continue
    }
    for (const v of versions) {
      if (!expected.has(v)) {
        problems.push(
          `${crate}@${v} ships, but BUNDLED_SOURCES was read at ${[...expected].join(', ')} — re-read the license of the sources it vendors and update the entry`,
        )
      }
    }
    for (const v of expected) {
      if (!versions.has(v)) problems.push(`BUNDLED_SOURCES names ${crate}@${v}, which no longer ships — update the entry`)
    }
  }

  for (const p of shipped) {
    if (SOURCE_CARRIER.test(p.name) && !carriers.has(p.name)) {
      problems.push(
        `${p.name}@${p.version} vendors sources into the build but is not in BUNDLED_SOURCES — read their license and add an entry`,
      )
    }
  }
  return [...new Set(problems)].sort()
}

// --- the texts -------------------------------------------------------------------------------------

const normalize = (text) =>
  text
    .replace(/^\uFEFF/, '')
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((l) => l.replace(/\s+$/, ''))
    .join('\n')
    .replace(/^\n+|\n+$/g, '')

// The license files a crate ships at its top level (plus its `license-file`, wherever that points).
export function licenseFiles(pkg, fs = { readdirSync, statSync, existsSync }) {
  const dir = dirname(pkg.manifest_path)
  const names = fs
    .readdirSync(dir)
    .filter((n) => LICENSE_FILE.test(n) && fs.statSync(join(dir, n)).isFile())
  if (pkg.license_file && !names.includes(pkg.license_file) && fs.existsSync(join(dir, pkg.license_file))) {
    names.push(pkg.license_file)
  }
  return names.sort().map((n) => join(dir, n))
}

// packages: every shipped package. read: path -> text. files: package -> license file paths.
// Returns the notices file as a string. Pure apart from what the two callbacks read.
export function render(packages, { read, files, table = BUNDLED_SOURCES }) {
  const lines = []
  const out = (...l) => lines.push(...l)

  out(
    'THIRD-PARTY NOTICES',
    '',
    'Amenbo is licensed under Apache-2.0 (see LICENSE). The binaries it distributes also contain',
    'the third-party software listed below, under the licenses reproduced here.',
    '',
    'This file is generated by scripts/gen-third-party-notices.mjs from Cargo.lock and',
    'app/src-tauri/Cargo.lock. Do not edit it by hand; run the script instead.',
    '',
  )

  // Bundled sources first: they are the part no crate declares.
  out('='.repeat(78), 'Sources compiled into the binaries from inside a Rust crate', '='.repeat(78), '')
  const byName = new Map([...packages].map((p) => [`${p.name}@${p.version}`, p]))
  for (const entry of table) {
    const pkg = byName.get(`${entry.crate}@${entry.version}`)
    if (!pkg) refuse(`${entry.crate}@${entry.version} (BUNDLED_SOURCES) is not shipped`)
    const dir = dirname(pkg.manifest_path)
    out(`${entry.component} — ${entry.license}, vendored by the ${entry.crate} crate`, '-'.repeat(78), '')
    if (entry.statement) {
      const source = read(join(dir, entry.evidence.file))
      if (source === null || !source.includes(entry.evidence.contains)) {
        refuse(`${entry.crate}@${entry.version}: ${entry.evidence.file} no longer says "${entry.evidence.contains}"`)
      }
      out(entry.statement, '')
    }
    for (const f of entry.files ?? []) {
      const text = read(join(dir, f))
      if (text === null) refuse(`${entry.crate}@${entry.version} has no ${f} — the vendored sources moved; update BUNDLED_SOURCES`)
      out(normalize(text), '')
    }
  }

  // Rust crates: an index, then each distinct license text once with the crates that ship it.
  const names = new Map() // name -> Set(declared license)
  const texts = new Map() // normalized text -> Set(name)
  const bare = new Map() // name -> declared license, for crates that ship no license file
  for (const p of packages) {
    const declared = p.license ?? (p.license_file ? `see ${p.license_file}` : 'undeclared')
    if (!names.has(p.name)) names.set(p.name, new Set())
    names.get(p.name).add(declared)

    const found = files(p)
    if (found.length === 0) bare.set(p.name, declared)
    for (const f of found) {
      const text = read(f)
      if (text === null || normalize(text) === '') continue
      const key = normalize(text)
      if (!texts.has(key)) texts.set(key, new Set())
      texts.get(key).add(p.name)
    }
  }

  out('='.repeat(78), 'Rust crates', '='.repeat(78), '')
  for (const name of [...names.keys()].sort()) out(`${name} — ${[...names.get(name)].sort().join('; ')}`)
  out('')

  if (bare.size > 0) {
    out(
      'The crates below ship no license file of their own; their declared license (above) applies,',
      'in the standard text reproduced for another crate under the same license in this file.',
      '',
    )
    for (const name of [...bare.keys()].sort()) out(`  ${name}`)
    out('')
  }

  out('='.repeat(78), 'License texts', '='.repeat(78), '')
  const groups = [...texts.entries()]
    .map(([text, users]) => ({ text, users: [...users].sort() }))
    .sort((a, b) => a.users[0].localeCompare(b.users[0]) || a.text.localeCompare(b.text))
  for (const { text, users } of groups) {
    out(`Used by: ${users.join(', ')}`, '-'.repeat(78), text, '')
  }

  return lines.join('\n').replace(/\n+$/, '\n')
}

// --- reading the tree -----------------------------------------------------------------------------

export function readMetadata(dir) {
  const json = execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], {
    cwd: join(ROOT, dir),
    encoding: 'utf8',
    maxBuffer: 1 << 30,
    stdio: ['ignore', 'pipe', 'inherit'],
  })
  return JSON.parse(json)
}

const readOrNull = (path) => {
  try {
    return readFileSync(path, 'utf8')
  } catch {
    return null
  }
}

// Every shipped package across the workspaces, one entry per name@version.
export function collectShipped(metadataFor = readMetadata) {
  const all = new Map()
  for (const ws of WORKSPACES) {
    for (const pkg of shippedPackages(metadataFor(ws.dir), ws.roots).values()) {
      all.set(`${pkg.name}@${pkg.version}`, pkg)
    }
  }
  return [...all.values()]
}

export function generate(packages = collectShipped()) {
  return render(packages, { read: readOrNull, files: (p) => licenseFiles(p) })
}

// Only when run as the generator. Imported (by the guard and its tests), this file does nothing.
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const packages = collectShipped()
    const problems = checkBundledSources(packages)
    if (problems.length > 0) {
      console.error('✗ the bundled sources no longer match BUNDLED_SOURCES:')
      for (const p of problems) console.error(`    ${p}`)
      process.exit(1)
    }
    writeFileSync(NOTICES, generate(packages))
    console.log(`→ wrote THIRD-PARTY-NOTICES (${packages.length} crates)`)
  } catch (e) {
    if (!(e instanceof NoticeError)) throw e
    console.error(`✗ ${e.message}`)
    process.exit(1)
  }
}
