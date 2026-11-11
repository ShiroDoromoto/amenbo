#!/usr/bin/env node
// gen-third-party-notices.mjs — write THIRD-PARTY-NOTICES, the license texts of what amenbo ships
// that amenbo did not write.
//
// The binaries statically link every Rust crate they depend on, so each crate's notice has to travel
// with them. `cargo deny check licenses` (deny.toml) judges whether a crate MAY ship; it does not
// carry the notice, and it only reads what a crate declares. That second half matters here: rusqlite
// is built with `bundled-sqlcipher-vendored-openssl`, so SQLCipher (BSD-3-Clause), SQLite (public
// domain) and OpenSSL (Apache-2.0) are compiled into the binaries from C sources vendored inside two
// crates — sqlcipher-src, whose top-level LICENSE is the crate's own MIT text while the SQLCipher
// and libtomcrypt texts sit next to the sources, and openssl-src, whose `MIT/Apache-2.0` is the
// crate's build glue, not OpenSSL. Reading a crate's top-level license files does not reach them, so
// BUNDLED_SOURCES below names them, by crate AND version: a bump of either carrier, or a new `*-src`
// crate, stops the gate until a human has looked at what the new sources say
// (guards/check-third-party-notices.mjs).
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
// The GUI also bundles npm packages, so their notices travel too. Which of them ship is decided by
// guards/check-npm-licenses.mjs (shippedPackages: app/package-lock.json minus dev, devOptional and
// link entries) — imported here, so the gate that judges a package and the file that carries its
// notice cannot disagree about the scope. The lockfile records each package's license but not its
// text, so the texts are read from app/node_modules: run `npm ci --ignore-scripts` in app/ first.
// Packages pinned to one os/cpu/libc are installed only on that platform; their files are not read,
// so the output does not depend on the machine it is generated on.
//
// The GUI also carries data that no package declares a license for: the TextMate grammars it takes
// out of `tm-grammars` one at a time, the language configurations and the grammar baked out of VS
// Code (app/src/files/langconfig, app/src/files/tmgrammar), and the character maps, fonts and
// decoders the PDF reader takes out of `pdfjs-dist`. Which of them ship and under what license is
// recorded in guards/check-grammar-licenses.mjs, the gate that judges them — imported here for the
// same reason as the npm half, so the two cannot disagree. Their texts are read from the same install.
//
// Usage: node scripts/gen-third-party-notices.mjs   (from anywhere; writes <repo>/THIRD-PARTY-NOTICES)

import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

import { GateError, NO_LICENSE_FIELD, shippedPackages as shippedNpmPackages } from '../guards/check-npm-licenses.mjs'
import { GRAMMARS, GRANTS, LANGCONFIG, PDFJS_DATA, TMGRAMMAR } from '../guards/check-grammar-licenses.mjs'

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
export const NOTICES = join(ROOT, 'THIRD-PARTY-NOTICES')
export const NPM_DIR = join(ROOT, 'app')

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
    crate: 'sqlcipher-src',
    version: '420.0.0+sqlcipher-4.20.0-sqlite-3.54.1-libtomcrypt-1.18.2',
    component: 'SQLCipher',
    license: 'BSD-3-Clause',
    files: ['sqlcipher/LICENSE-sqlcipher'],
  },
  {
    crate: 'sqlcipher-src',
    version: '420.0.0+sqlcipher-4.20.0-sqlite-3.54.1-libtomcrypt-1.18.2',
    component: 'SQLite (inside the SQLCipher amalgamation)',
    license: 'blessing (public domain)',
    evidence: { file: 'sqlcipher/sqlcipher.c', contains: 'The author disclaims copyright to this source code.' },
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
    crate: 'sqlcipher-src',
    version: '420.0.0+sqlcipher-4.20.0-sqlite-3.54.1-libtomcrypt-1.18.2',
    component: 'LibTomCrypt',
    license: 'public domain or WTFPL',
    files: ['sqlcipher/LICENSE-libtomcrypt'],
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

// packages: every shipped crate. npm: every shipped npm package, as collectNpm returns them.
// data: every piece of bundled data, as collectData returns them.
// read: path -> text. files: crate -> license file paths.
// Returns the notices file as a string. Pure apart from what the two callbacks read.
export function render(packages, { read, files, table = BUNDLED_SOURCES, npm = [], data = [] }) {
  const lines = []
  const out = (...l) => lines.push(...l)

  out(
    'THIRD-PARTY NOTICES',
    '',
    'Amenbo is licensed under Apache-2.0 (see LICENSE). The binaries it distributes also contain',
    'the third-party software listed below, under the licenses reproduced here.',
    '',
    'This file is generated by scripts/gen-third-party-notices.mjs from Cargo.lock,',
    'app/src-tauri/Cargo.lock, app/package-lock.json and the records of bundled data in',
    'guards/check-grammar-licenses.mjs. Do not edit it by hand; run the script instead.',
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

  const crates = [...packages].map((p) => ({
    name: p.name,
    declared: p.license ?? (p.license_file ? `see ${p.license_file}` : 'undeclared'),
    files: files(p),
  }))
  section(out, read, { title: 'Rust crates', noun: 'crate', entries: crates })
  section(out, read, { title: 'npm packages (bundled into the GUI)', noun: 'package', entries: npm })
  section(out, read, { title: 'Data bundled into the GUI (editor grammars and configurations, PDF reader data)', noun: 'file', entries: data })

  return lines.join('\n').replace(/\n+$/, '\n')
}

// One ecosystem's part of the file: an index, then each distinct license text once with the names
// that ship it. entries: { name, declared, files, texts, platform } — `texts` are license texts
// that come from somewhere other than a file of their own (see collectData), and `platform` marks a
// package whose files were deliberately not read (see collectNpm).
function section(out, read, { title, noun, entries }) {
  const names = new Map() // name -> Set(declared license)
  const texts = new Map() // normalized text -> Set(name)
  const bare = new Set() // names that ship no license file
  const platform = new Set() // names whose files were not read
  for (const p of entries) {
    if (!names.has(p.name)) names.set(p.name, new Set())
    names.get(p.name).add(p.declared)

    if (p.platform) {
      platform.add(p.name)
      continue
    }
    const found = [...p.files.map(read), ...(p.texts ?? [])]
    if (found.length === 0) bare.add(p.name)
    for (const text of found) {
      if (text === null || normalize(text) === '') continue
      const key = normalize(text)
      if (!texts.has(key)) texts.set(key, new Set())
      texts.get(key).add(p.name)
    }
  }

  out('='.repeat(78), title, '='.repeat(78), '')
  for (const name of [...names.keys()].sort()) out(`${name} — ${[...names.get(name)].sort().join('; ')}`)
  out('')

  if (bare.size > 0) {
    out(
      `The ${noun}s below ship no license file of their own; their declared license (above) applies,`,
      `in the standard text reproduced for another ${noun} under the same license in this file.`,
      '',
    )
    for (const name of [...bare].sort()) out(`  ${name}`)
    out('')
  }

  if (platform.size > 0) {
    out(
      `The ${noun}s below are prebuilt for one platform each and installed only there, so their files`,
      'are not read here; their declared license (above) applies.',
      '',
    )
    for (const name of [...platform].sort()) out(`  ${name}`)
    out('')
  }

  out('='.repeat(78), `License texts — ${title}`, '='.repeat(78), '')
  const groups = [...texts.entries()]
    .map(([text, users]) => ({ text, users: [...users].sort() }))
    .sort((a, b) => a.users[0].localeCompare(b.users[0]) || a.text.localeCompare(b.text))
  for (const { text, users } of groups) {
    out(`Used by: ${users.join(', ')}`, '-'.repeat(78), text, '')
  }
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

export const readNpmLock = () => JSON.parse(readFileSync(join(NPM_DIR, 'package-lock.json'), 'utf8'))

// The regular files directly inside dir, or null when dir is not there.
const filesIn = (dir) => {
  try {
    return readdirSync(dir).filter((n) => statSync(join(dir, n)).isFile())
  } catch {
    return null
  }
}

// lock: app/package-lock.json, parsed. Returns one entry per shipped package path, with the license
// files found at the top of its installed directory. ls: dir -> file names (or null); read: path -> text.
export function collectNpm(lock, { dir = NPM_DIR, ls = filesIn, read = readOrNull } = {}) {
  const entries = []
  for (const [path, p] of shippedNpmPackages(lock)) {
    // The gate refuses a package with neither a license field nor a NO_LICENSE_FIELD entry.
    const declared = p.license ?? NO_LICENSE_FIELD[p.name]?.license ?? 'undeclared'
    if (p.platform) {
      entries.push({ name: p.name, declared, files: [], platform: true })
      continue
    }
    const pkgDir = join(dir, path)
    const manifest = read(join(pkgDir, 'package.json'))
    if (manifest === null) refuse(`app/${path} is not installed — run npm ci --ignore-scripts in app/`)
    const installed = JSON.parse(manifest).version
    if (installed !== p.version) {
      refuse(`app/${path} is ${installed}, but package-lock.json says ${p.version} — run npm ci --ignore-scripts in app/`)
    }
    const files = (ls(pkgDir) ?? []).filter((n) => LICENSE_FILE.test(n)).sort().map((n) => join(pkgDir, n))
    entries.push({ name: p.name, declared, files, platform: false })
  }
  return entries
}

// tm-grammars' NOTICE, cut into its blocks: each names the grammar files it covers, the license
// they came under (as a URL and an SPDX identifier) and that license's text.
export function noticeBlocks(text) {
  const blocks = []
  for (const chunk of normalize(text).split(/^={20,}$/m).slice(1)) {
    const at = chunk.search(/^-{20,}$/m)
    if (at < 0) continue
    const head = chunk.slice(0, at)
    const field = (name) => head.match(new RegExp(`^${name}:[ \\t]*(.*)$`, 'm'))?.[1].trim()
    blocks.push({
      files: (field('Files') ?? '').split(',').map((f) => f.trim()).filter(Boolean),
      url: field('License') ?? '',
      spdx: field('SPDX'),
      text: normalize(chunk.slice(at).replace(/^-+\n/, '')),
    })
  }
  return blocks
}

// Every piece of data the GUI bundles that no package declares, one entry per file (or per glob, for
// the character maps), from the records guards/check-grammar-licenses.mjs judges. That gate holds
// the records to what the GUI actually bundles; this only carries their texts, and refuses where the
// record and the installed files no longer agree.
//
// - A grammar with an SPDX license carries the text tm-grammars' NOTICE gives for that file, and
//   that text has to be under the license the record says. A grammar under a grant carries the
//   grant as the record quotes it.
// - The baked trees are VS Code's, under the license their SOURCE.json records. VS Code's license
//   text is not in the tree; tm-grammars' NOTICE carries it, verbatim, for the grammars it took out
//   of the same repository, so that copy is the one reproduced.
// - pdf.js's data carries the license files the record names inside pdfjs-dist, which has to be
//   installed at the version they were read at.
export function collectData({
  read = readOrNull,
  root = ROOT,
  npmDir = NPM_DIR,
  langconfig = LANGCONFIG,
  tmgrammar = TMGRAMMAR,
  grammars = GRAMMARS,
  grants = GRANTS,
  pdfjs = PDFJS_DATA,
} = {}) {
  const entries = []
  const notice = read(join(npmDir, 'node_modules', 'tm-grammars', 'NOTICE'))
  if (notice === null) refuse('app/node_modules/tm-grammars/NOTICE is not there — run npm ci --ignore-scripts in app/')
  const blocks = noticeBlocks(notice)

  for (const id of Object.keys(grammars).sort()) {
    for (const { grammar, license, grant, source } of grammars[id]) {
      const name = `tm-grammars: ${grammar}.json`
      if (grant !== undefined) {
        const known = grants[grant]
        if (known === undefined) refuse(`${grammar} names the grant "${grant}", which guards/check-grammar-licenses.mjs does not record`)
        entries.push({ name, declared: `${grant} (no SPDX identifier), read at ${source}`, files: [], texts: [`${known.text}\n\n(${known.from})`] })
        continue
      }
      const block = blocks.find((b) => b.files.includes(`${grammar}.json`))
      if (block === undefined) refuse(`tm-grammars' NOTICE names no license for ${grammar}.json — re-read its source and update GRAMMARS`)
      if (block.spdx !== license) {
        refuse(`tm-grammars' NOTICE says ${grammar}.json is ${block.spdx}, but GRAMMARS records ${license} — re-read its source`)
      }
      entries.push({ name, declared: `${license}, read at ${source}`, files: [], texts: [block.text] })
    }
  }

  for (const [dir, key] of [[langconfig, 'lang'], [tmgrammar, 'scope']]) {
    const where = relative(root, dir)
    const raw = read(join(dir, 'SOURCE.json'))
    if (raw === null) refuse(`${where}/SOURCE.json is missing`)
    const manifest = JSON.parse(raw)
    const repo = `${manifest.repository.replace(/^https:\/\/github\.com\//, 'https://raw.githubusercontent.com/')}/`
    const block = blocks.find((b) => b.url.startsWith(repo) && b.spdx === manifest.licence)
    if (block === undefined) refuse(`tm-grammars' NOTICE no longer carries the ${manifest.licence} text of ${manifest.repository}, which ${where} ships under`)
    for (const f of manifest.files ?? []) {
      entries.push({
        name: `${where}/${f[key]}.json`,
        declared: `${manifest.licence}, from ${manifest.repository} at ${manifest.tag} (${manifest.revision})`,
        files: [],
        texts: [block.text],
      })
    }
  }

  const pdfjsDir = join(npmDir, 'node_modules', 'pdfjs-dist')
  const manifest = read(join(pdfjsDir, 'package.json'))
  if (manifest === null) refuse('app/node_modules/pdfjs-dist is not installed — run npm ci --ignore-scripts in app/')
  const installed = JSON.parse(manifest).version
  if (installed !== pdfjs.version) {
    refuse(`app/node_modules/pdfjs-dist is ${installed}, but PDFJS_DATA was read at ${pdfjs.version} — re-read its license files`)
  }
  for (const [path, { license, texts }] of Object.entries(pdfjs.files)) {
    for (const t of texts) {
      if (read(join(pdfjsDir, t)) === null) refuse(`pdfjs-dist ${installed} has no ${t} — update PDFJS_DATA`)
    }
    entries.push({ name: `pdfjs-dist: ${path}`, declared: license, files: texts.map((t) => join(pdfjsDir, t)) })
  }
  return entries
}

export function generate(packages = collectShipped(), npm = collectNpm(readNpmLock()), data = collectData()) {
  return render(packages, { read: readOrNull, files: (p) => licenseFiles(p), npm, data })
}

// The npm half refuses with the license gate's own error; both mean "stop and say why".
export const isRefusal = (e) => e instanceof NoticeError || e instanceof GateError

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
    const npm = collectNpm(readNpmLock())
    const data = collectData()
    writeFileSync(NOTICES, generate(packages, npm, data))
    console.log(`→ wrote THIRD-PARTY-NOTICES (${packages.length} crates, ${npm.length} npm packages, ${data.length} bundled data files)`)
  } catch (e) {
    if (!isRefusal(e)) throw e
    console.error(`✗ ${e.message}`)
    process.exit(1)
  }
}
