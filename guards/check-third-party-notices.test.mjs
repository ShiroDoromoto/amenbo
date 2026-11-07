// Tests for the third-party notices gate.
//
// A real run only exercises the tree we have, which is green — so it shows the gate can say yes,
// never that it can still say no. These pin the refusals: a carrier of vendored C sources bumped
// without anyone re-reading them, a new `*-src` crate nobody looked at, vendored license files that
// moved, a test-only crate counted as shipped, a dev-only npm package counted as shipped, an npm
// tree that is not installed as the lockfile says, bundled data whose record and installed files
// disagree, and a committed file that drifted.
//
// Run: node --test guards/
//
// Everything here is pure: the metadata, the crate sources and the license texts are synthetic.

import { test } from 'node:test'
import assert from 'node:assert/strict'

import {
  checkBundledSources,
  collectData,
  collectNpm,
  noticeBlocks,
  render,
  shippedPackages,
  NoticeError,
} from '../scripts/gen-third-party-notices.mjs'
import { firstDifference } from './check-third-party-notices.mjs'

// --- the shipped tree ---------------------------------------------------------------------------

const normal = [{ kind: null, target: null }]
const build = [{ kind: 'build', target: null }]
const dev = [{ kind: 'dev', target: null }]

const pkg = (name, version = '1.0.0', extra = {}) => ({
  id: `${name}@${version}`,
  name,
  version,
  source: extra.local ? null : 'registry+https://github.com/rust-lang/crates.io-index',
  license: 'MIT',
  manifest_path: `/reg/${name}-${version}/Cargo.toml`,
  ...extra,
})

function metadata() {
  return {
    packages: [
      pkg('amenbo-cli', '1.0.0', { local: true }),
      pkg('amenbo-scratch', '1.0.0', { local: true }),
      pkg('serde'),
      pkg('openssl-sys'),
      pkg('openssl-src', '300.0.0'),
      pkg('tempfile'),
    ],
    resolve: {
      nodes: [
        {
          id: 'amenbo-cli@1.0.0',
          deps: [
            { pkg: 'serde@1.0.0', dep_kinds: normal },
            { pkg: 'openssl-sys@1.0.0', dep_kinds: normal },
            { pkg: 'amenbo-scratch@1.0.0', dep_kinds: dev },
          ],
        },
        { id: 'openssl-sys@1.0.0', deps: [{ pkg: 'openssl-src@300.0.0', dep_kinds: build }] },
        { id: 'amenbo-scratch@1.0.0', deps: [{ pkg: 'tempfile@1.0.0', dep_kinds: normal }] },
        { id: 'serde@1.0.0', deps: [] },
        { id: 'openssl-src@300.0.0', deps: [] },
        { id: 'tempfile@1.0.0', deps: [] },
      ],
    },
  }
}

const names = (map) => [...map.values()].map((p) => p.name).sort()

test('normal and build edges ship; dev edges and our own crates do not', () => {
  assert.deepEqual(names(shippedPackages(metadata(), ['amenbo-cli'])), ['openssl-src', 'openssl-sys', 'serde'])
})

test('a root that is not in the workspace stops the gate instead of shipping nothing', () => {
  assert.throws(() => shippedPackages(metadata(), ['amenbo-gone']), NoticeError)
})

test('an empty resolve graph is refused rather than passed', () => {
  assert.throws(() => shippedPackages({ ...metadata(), resolve: { nodes: [] } }, ['amenbo-cli']), NoticeError)
})

// --- the bundled C sources ------------------------------------------------------------------------

const TABLE = [{ crate: 'openssl-src', version: '300.0.0', component: 'OpenSSL', license: 'Apache-2.0', files: ['openssl/LICENSE.txt'] }]
const shipped = (...list) => list.map(([name, version]) => pkg(name, version))

test('the carrier at the version its entry was read at passes', () => {
  assert.deepEqual(checkBundledSources(shipped(['openssl-src', '300.0.0'], ['serde', '1.0.0']), TABLE), [])
})

test('a bumped carrier is refused until someone re-reads its sources', () => {
  const problems = checkBundledSources(shipped(['openssl-src', '300.1.0']), TABLE)
  assert.equal(problems.length, 2)
  assert.match(problems.join('\n'), /openssl-src@300\.1\.0 ships/)
  assert.match(problems.join('\n'), /openssl-src@300\.0\.0, which no longer ships/)
})

test('a carrier that disappeared is refused: its sources moved somewhere unnamed', () => {
  assert.match(checkBundledSources(shipped(['serde', '1.0.0']), TABLE).join('\n'), /no longer ships/)
})

test('a new *-src crate is refused until it has an entry', () => {
  const problems = checkBundledSources(shipped(['openssl-src', '300.0.0'], ['sqlcipher-src', '419.0.6']), TABLE)
  assert.match(problems.join('\n'), /sqlcipher-src@419\.0\.6 vendors sources/)
})

// --- the rendered file ----------------------------------------------------------------------------

const FILES = {
  '/reg/openssl-src-300.0.0/openssl/LICENSE.txt': 'Apache License\nVersion 2.0\n',
  '/reg/serde-1.0.0/LICENSE-MIT': 'MIT text\r\n',
  '/reg/itoa-1.0.0/LICENSE-MIT': 'MIT text\n',
}
const read = (p) => FILES[p] ?? null
const files = (p) => Object.keys(FILES).filter((f) => f.startsWith(`/reg/${p.name}-${p.version}/`) && !f.includes('/openssl/'))

test('vendored texts lead, and identical crate texts are written once', () => {
  const out = render(shipped(['openssl-src', '300.0.0'], ['serde', '1.0.0'], ['itoa', '1.0.0']), { read, files, table: TABLE })
  assert.match(out, /OpenSSL — Apache-2.0, vendored by the openssl-src crate/)
  assert.ok(out.indexOf('Apache License') < out.indexOf('Rust crates'))
  assert.match(out, /Used by: itoa, serde\n-+\nMIT text\n/)
  assert.equal(out.match(/MIT text/g).length, 1)
})

test('a vendored license file that moved stops the generator', () => {
  const moved = { ...TABLE[0], files: ['openssl/LICENSE'] }
  assert.throws(() => render(shipped(['openssl-src', '300.0.0']), { read, files, table: [moved] }), NoticeError)
})

test('a statement whose evidence left the sources stops the generator', () => {
  const entry = { ...TABLE[0], files: [], statement: 'blessing', evidence: { file: 'sqlite3.c', contains: 'disclaims' } }
  assert.throws(() => render(shipped(['openssl-src', '300.0.0']), { read, files, table: [entry] }), NoticeError)
})

// --- the npm packages -----------------------------------------------------------------------------

const lock = () => ({
  lockfileVersion: 3,
  packages: {
    '': { name: 'amenbo-app' },
    'node_modules/dompurify': { version: '3.0.0', license: '(MPL-2.0 OR Apache-2.0)' },
    'node_modules/elkjs': { version: '0.1.0', license: 'EPL-2.0 OR GPL-3.0-or-later' },
    'node_modules/khroma': { version: '2.0.0' },
    'node_modules/a/node_modules/khroma': { version: '1.0.0' },
    'node_modules/canvas-linux-x64': { version: '1.0.0', license: 'MIT', optional: true, os: ['linux'], cpu: ['x64'] },
    'node_modules/vite': { version: '7.0.0', license: 'MIT', dev: true },
    'node_modules/a': { version: '1.0.0', license: 'MIT' },
  },
})

const NPM_FILES = {
  '/app/node_modules/dompurify/package.json': '{"version":"3.0.0"}',
  '/app/node_modules/dompurify/LICENSE': 'Apache License\nVersion 2.0\n',
  '/app/node_modules/dompurify/LICENSE-MPL': 'Mozilla Public License Version 2.0\n',
  '/app/node_modules/elkjs/package.json': '{"version":"0.1.0"}',
  '/app/node_modules/elkjs/LICENSE.md': 'Eclipse Public License - v 2.0\n',
  '/app/node_modules/khroma/package.json': '{"version":"2.0.0"}',
  '/app/node_modules/khroma/license': 'MIT text\n',
  '/app/node_modules/a/node_modules/khroma/package.json': '{"version":"1.0.0"}',
  '/app/node_modules/a/node_modules/khroma/license': 'MIT text\n',
  '/app/node_modules/a/package.json': '{"version":"1.0.0"}',
  '/app/node_modules/a/README.md': 'not a license\n',
}
const npmRead = (files) => (p) => files[p] ?? null
const npmLs = (files) => (dir) => {
  const names = Object.keys(files)
    .filter((f) => f.startsWith(`${dir}/`) && !f.slice(dir.length + 1).includes('/'))
    .map((f) => f.slice(dir.length + 1))
  return names.length > 0 ? names : null
}
const collect = (files = NPM_FILES, l = lock()) => collectNpm(l, { dir: '/app', ls: npmLs(files), read: npmRead(files) })

test('the npm side ships what the license gate judges: dev packages stay out', () => {
  const names = collect().map((e) => e.name)
  assert.ok(!names.includes('vite'))
  assert.deepEqual([...new Set(names)].sort(), ['a', 'canvas-linux-x64', 'dompurify', 'elkjs', 'khroma'])
})

test("a package with no license field is declared under the gate's recorded verdict", () => {
  assert.ok(collect().filter((e) => e.name === 'khroma').every((e) => e.declared === 'MIT'))
})

test('a package pinned to one platform is listed but its files are not read', () => {
  const entry = collect().find((e) => e.name === 'canvas-linux-x64')
  assert.deepEqual(entry, { name: 'canvas-linux-x64', declared: 'MIT', files: [], platform: true })
})

test('a shipped package missing from node_modules stops the generator', () => {
  const { ['/app/node_modules/elkjs/package.json']: _, ...missing } = NPM_FILES
  assert.throws(() => collect(missing), (e) => e instanceof NoticeError && /elkjs is not installed/.test(e.message))
})

test('node_modules at another version than the lockfile stops the generator', () => {
  const stale = { ...NPM_FILES, '/app/node_modules/elkjs/package.json': '{"version":"0.0.9"}' }
  assert.throws(() => collect(stale), (e) => e instanceof NoticeError && /0\.0\.9, but package-lock\.json says 0\.1\.0/.test(e.message))
})

test('npm texts follow the crates, MPL and EPL texts are kept, and identical texts are written once', () => {
  const out = render(shipped(['openssl-src', '300.0.0'], ['serde', '1.0.0']), { read: (p) => FILES[p] ?? NPM_FILES[p] ?? null, files, table: TABLE, npm: collect() })
  assert.ok(out.indexOf('License texts — Rust crates') < out.indexOf('npm packages (bundled into the GUI)'))
  assert.match(out, /dompurify — \(MPL-2\.0 OR Apache-2\.0\)/)
  assert.match(out, /Used by: dompurify\n-+\nMozilla Public License Version 2\.0\n/)
  assert.match(out, /Used by: elkjs\n-+\nEclipse Public License - v 2\.0\n/)
  assert.match(out, /Used by: khroma\n-+\nMIT text\n/)
  assert.match(out, /ship no license file of their own[^]*\n {2}a\n/)
  assert.match(out, /prebuilt for one platform[^]*\n {2}canvas-linux-x64\n/)
  assert.ok(!out.includes('not a license'))
})

// --- the bundled data ------------------------------------------------------------------------------

const RULE = '='.repeat(40)
const DASH = '-'.repeat(40)
const TM_NOTICE = [
  'THIRD-PARTY SOFTWARE NOTICES',
  '',
  RULE,
  'Files:   css.json, rust.json, html.json',
  'License: https://raw.githubusercontent.com/example/editor/main/LICENSE.txt',
  'SPDX:    MIT',
  DASH,
  'Editor MIT text',
  '---',
  'still the same text',
  '',
  RULE,
  'Files:   cobol.json',
  'License: https://example.invalid/LICENSE',
  'SPDX:    GPL-3.0',
  DASH,
  'GPL text',
].join('\n')

const SOURCE = (key, names) =>
  JSON.stringify({
    repository: 'https://github.com/example/editor',
    licence: 'MIT',
    tag: '1.0.0',
    revision: 'a'.repeat(40),
    files: names.map((n) => ({ [key]: n })),
  })

const DATA_FILES = {
  '/app/node_modules/tm-grammars/NOTICE': TM_NOTICE,
  '/tree/langconfig/SOURCE.json': SOURCE('lang', ['css']),
  '/tree/tmgrammar/SOURCE.json': SOURCE('scope', ['text.html.php']),
  '/app/node_modules/pdfjs-dist/package.json': '{"version":"1.0.0"}',
  '/app/node_modules/pdfjs-dist/cmaps/LICENSE': 'Adobe BSD text\n',
  '/app/node_modules/pdfjs-dist/wasm/LICENSE_OPENJPEG': 'OpenJPEG BSD text\n',
}
const DATA = {
  root: '/tree',
  npmDir: '/app',
  langconfig: '/tree/langconfig',
  tmgrammar: '/tree/tmgrammar',
  grammars: {
    rust: [{ grammar: 'rust', license: 'MIT', source: 'https://example.invalid/rust' }],
    yaml: [{ grammar: 'yaml', grant: 'open', source: 'https://example.invalid/yaml' }],
  },
  grants: { open: { from: 'https://example.invalid/terms', text: 'Anything goes.' } },
  pdfjs: {
    version: '1.0.0',
    files: {
      'cmaps/*.bcmap': { license: 'BSD-3-Clause', texts: ['cmaps/LICENSE'] },
      'wasm/openjpeg_nowasm_fallback.js': { license: 'BSD-2-Clause', texts: ['wasm/LICENSE_OPENJPEG'] },
    },
  },
}
const data = (files = DATA_FILES, over = {}) => collectData({ read: (p) => files[p] ?? null, ...DATA, ...over })

test("tm-grammars' NOTICE is cut into its blocks, a short rule inside a text included", () => {
  const blocks = noticeBlocks(TM_NOTICE)
  assert.equal(blocks.length, 2)
  assert.deepEqual(blocks[0].files, ['css.json', 'rust.json', 'html.json'])
  assert.equal(blocks[0].spdx, 'MIT')
  assert.equal(blocks[0].text, 'Editor MIT text\n---\nstill the same text')
})

test('every recorded grammar, baked file and pdf.js file is one entry', () => {
  assert.deepEqual(data().map((e) => e.name), [
    'tm-grammars: rust.json',
    'tm-grammars: yaml.json',
    'langconfig/css.json',
    'tmgrammar/text.html.php.json',
    'pdfjs-dist: cmaps/*.bcmap',
    'pdfjs-dist: wasm/openjpeg_nowasm_fallback.js',
  ])
})

test('a grammar carries the NOTICE text for its file; a grant carries the grant as recorded', () => {
  const [rust, yaml] = data()
  assert.deepEqual(rust.texts, ['Editor MIT text\n---\nstill the same text'])
  assert.match(rust.declared, /^MIT, read at https:\/\/example\.invalid\/rust$/)
  assert.deepEqual(yaml.texts, ['Anything goes.\n\n(https://example.invalid/terms)'])
})

test("the baked trees carry their repository's text, under the licence their manifest records", () => {
  const css = data().find((e) => e.name === 'langconfig/css.json')
  assert.deepEqual(css.texts, ['Editor MIT text\n---\nstill the same text'])
  assert.match(css.declared, /MIT, from https:\/\/github\.com\/example\/editor at 1\.0\.0/)
})

test('a grammar the NOTICE puts under another licence than its record stops the generator', () => {
  const grammars = { cobol: [{ grammar: 'cobol', license: 'MIT', source: 'https://example.invalid/cobol' }] }
  assert.throws(() => data(DATA_FILES, { grammars }), (e) => e instanceof NoticeError && /cobol\.json is GPL-3\.0/.test(e.message))
})

test('a grammar the NOTICE does not name stops the generator', () => {
  const grammars = { go: [{ grammar: 'go', license: 'MIT', source: 'https://example.invalid/go' }] }
  assert.throws(() => data(DATA_FILES, { grammars }), (e) => e instanceof NoticeError && /names no license for go\.json/.test(e.message))
})

test('pdfjs-dist installed at another version than its record stops the generator', () => {
  const bumped = { ...DATA_FILES, '/app/node_modules/pdfjs-dist/package.json': '{"version":"1.0.1"}' }
  assert.throws(() => data(bumped), (e) => e instanceof NoticeError && /1\.0\.1, but PDFJS_DATA was read at 1\.0\.0/.test(e.message))
})

test('a pdf.js licence file that moved stops the generator', () => {
  const { ['/app/node_modules/pdfjs-dist/wasm/LICENSE_OPENJPEG']: _, ...moved } = DATA_FILES
  assert.throws(() => data(moved), (e) => e instanceof NoticeError && /no wasm\/LICENSE_OPENJPEG/.test(e.message))
})

test('the bundled data follows the npm packages, and each text is written once', () => {
  const read = (p) => FILES[p] ?? DATA_FILES[p] ?? null
  const out = render(shipped(['openssl-src', '300.0.0']), { read, files, table: TABLE, data: data() })
  assert.ok(out.indexOf('npm packages (bundled into the GUI)') < out.indexOf('Data bundled into the GUI'))
  assert.match(out, /pdfjs-dist: wasm\/openjpeg_nowasm_fallback\.js — BSD-2-Clause/)
  assert.match(out, /Used by: langconfig\/css\.json, tm-grammars: rust\.json, tmgrammar\/text\.html\.php\.json\n-+\nEditor MIT text\n/)
  assert.match(out, /Used by: pdfjs-dist: cmaps\/\*\.bcmap\n-+\nAdobe BSD text\n/)
  assert.equal(out.match(/Editor MIT text/g).length, 1)
})

test('a committed file that drifted is reported at its first differing line', () => {
  assert.equal(firstDifference('a\nb\n', 'a\nb\n'), null)
  assert.deepEqual(firstDifference('a\nb\n', 'a\nc\n'), { line: 2, committed: 'b', fresh: 'c' })
  assert.deepEqual(firstDifference('a', 'a\nb'), { line: 2, committed: '(end of file)', fresh: 'b' })
})
