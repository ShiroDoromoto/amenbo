// Tests for the third-party notices gate.
//
// A real run only exercises the tree we have, which is green — so it shows the gate can say yes,
// never that it can still say no. These pin the refusals: a carrier of vendored C sources bumped
// without anyone re-reading them, a new `*-src` crate nobody looked at, vendored license files that
// moved, a test-only crate counted as shipped, and a committed file that drifted.
//
// Run: node --test guards/
//
// Everything here is pure: the metadata, the crate sources and the license texts are synthetic.

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { checkBundledSources, render, shippedPackages, NoticeError } from '../scripts/gen-third-party-notices.mjs'
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

test('a committed file that drifted is reported at its first differing line', () => {
  assert.equal(firstDifference('a\nb\n', 'a\nb\n'), null)
  assert.deepEqual(firstDifference('a\nb\n', 'a\nc\n'), { line: 2, committed: 'b', fresh: 'c' })
  assert.deepEqual(firstDifference('a', 'a\nb'), { line: 2, committed: '(end of file)', fresh: 'b' })
})
