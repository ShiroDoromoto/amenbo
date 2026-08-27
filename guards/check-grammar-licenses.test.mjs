// Tests for the grammar license gate.
//
// Same argument as its sibling's tests: a real run only exercises the catalog we HAVE, which is
// green, so it shows the gate can say yes and never that it can still say no. Every case below is
// a way it must refuse — a grammar bundled with no licence recorded, a copyleft one, a stale entry,
// and a grant nobody claims.
//
// Run: node --test guards/

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { bundledGrammars, judgeGrammars } from './check-grammar-licenses.mjs'

const ALLOW = new Set(['MIT', 'Apache-2.0'])
const GRANTS = { open: { from: 'https://example.invalid', text: 'anything goes', why: 'no condition attached' } }
const TABLE = {
  rust: [{ grammar: 'rust', license: 'MIT', source: 'https://example.invalid/rust' }],
  html: [
    { grammar: 'html', license: 'MIT', source: 'https://example.invalid/html' },
    { grammar: 'css', license: 'MIT', source: 'https://example.invalid/css' },
  ],
}
const judge = (ids, table = TABLE, grants = {}) => judgeGrammars(new Set(ids), ALLOW, table, grants)

// --- reading the catalog -----------------------------------------------------------------------

test('the bundled set is the import specifiers, whatever the map around them is called', () => {
  const source = `
    export const GRAMMARS = {
      rust: () => import("@shikijs/langs/rust"),
      tsx: () => import('@shikijs/langs/tsx'),
    }
    export const SCOPES = { rust: "source.rust" }
  `
  assert.deepEqual([...bundledGrammars(source)].sort(), ['rust', 'tsx'])
})

// A catalog that stopped importing grammars is a catalog that moved. Answering "nothing is
// bundled, all clear" would be the one wrong answer: the gate would go green forever.
test('a catalog with no grammar in it stops the gate rather than passing vacuously', () => {
  assert.throws(() => bundledGrammars('export const GRAMMARS = {}'), /catalog moved/)
})

// --- the judgment ------------------------------------------------------------------------------

test('a grammar bundled with no licence recorded is refused', () => {
  const { violations } = judge(['rust', 'html', 'ahk2'])
  assert.equal(violations.length, 1)
  assert.match(violations[0], /ahk2 is bundled but no licence is recorded/)
})

// The reason this gate exists: `@shikijs/langs` declares MIT and ships GPL-3.0 grammars inside it,
// so the package's own field cannot be what a grammar is judged by.
test('a copyleft grammar is refused even though its package declares MIT', () => {
  const table = { ...TABLE, gnuplot: [{ grammar: 'gnuplot', license: 'GPL-3.0', source: 'https://example.invalid/gnuplot' }] }
  const { violations } = judge(['rust', 'html', 'gnuplot'], table)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /gnuplot is GPL-3\.0/)
})

test('a grammar carried inside another is judged too', () => {
  const table = { html: [
    { grammar: 'html', license: 'MIT', source: 'https://example.invalid/html' },
    { grammar: 'css', license: 'AGPL-3.0', source: 'https://example.invalid/css' },
  ] }
  const { violations } = judge(['html'], table)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /css is AGPL-3\.0/)
})

test('an entry for a grammar nobody bundles any more is refused', () => {
  const { violations } = judge(['rust'])
  assert.equal(violations.length, 1)
  assert.match(violations[0], /html is recorded here but is no longer bundled/)
})

test('a grammar whose terms have no SPDX identifier passes on a recorded grant', () => {
  const table = { yaml: [{ grammar: 'yaml', grant: 'open', source: 'https://example.invalid/yaml' }] }
  const { violations, usedGrants } = judgeGrammars(new Set(['yaml']), ALLOW, table, GRANTS)
  assert.deepEqual(violations, [])
  assert.deepEqual([...usedGrants], ['open'])
})

test('a grant that is named but not recorded is refused', () => {
  const table = { yaml: [{ grammar: 'yaml', grant: 'handshake', source: 'https://example.invalid/yaml' }] }
  const { violations } = judgeGrammars(new Set(['yaml']), ALLOW, table, GRANTS)
  assert.equal(violations.length, 2)
  assert.match(violations[0], /names the grant "handshake"/)
})

// The other half of an exception. A grant nothing claims is green either way, so nothing would
// notice it went stale — and the one door around the allow-list is the last one to leave ajar.
test('a grant nothing claims is refused', () => {
  const table = { rust: TABLE.rust }
  const { violations } = judgeGrammars(new Set(['rust']), ALLOW, table, GRANTS)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /the grant "open" is recorded here but nothing claims it/)
})

test('the catalog we actually ship passes', async () => {
  const { violations } = judgeGrammars(
    bundledGrammars(await import('node:fs').then((fs) => fs.readFileSync('app/src/files/grammars.ts', 'utf8'))),
    new Set(['MIT']),
  )
  assert.deepEqual(violations, [])
})
