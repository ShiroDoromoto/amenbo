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

import { bundledGrammars, judgeGrammars, judgeLangConfig, judgeTmGrammar } from './check-grammar-licenses.mjs'

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
      rust: () => [import("tm-grammars/grammars/rust.json")],
      tsx: () => [import('tm-grammars/grammars/tsx.json')],
    }
    export const SCOPES = { rust: "source.rust" }
  `
  assert.deepEqual([...bundledGrammars(source)].sort(), ['rust', 'tsx'])
})

// A language is drawn with the grammars its catalog names, and every one of them is bundled — which
// is what the gate has to judge. Reading the keys would have missed the two behind `html`.
test('every grammar a language is drawn with is read, not just the language', () => {
  const source = `
    export const GRAMMARS = {
      html: () => [
        import("tm-grammars/grammars/html.json"),
        import("tm-grammars/grammars/javascript.json"),
        import("tm-grammars/grammars/css.json"),
      ],
    }
  `
  assert.deepEqual([...bundledGrammars(source)].sort(), ['css', 'html', 'javascript'])
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

// The reason this gate exists: `tm-grammars` declares MIT and ships GPL-3.0 grammars inside it,
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

// --- the baked language configurations ----------------------------------------------------------

const MANIFEST = {
  licence: 'MIT',
  repository: 'https://github.com/microsoft/vscode',
  revision: 'a'.repeat(40),
  files: [{ lang: 'rust', source: `https://example.invalid/${'a'.repeat(40)}/rust` }],
}

test('a baked configuration set that matches its manifest passes', () => {
  const { violations, judged } = judgeLangConfig(MANIFEST, ['rust.json'], ALLOW)
  assert.deepEqual(violations, [])
  assert.equal(judged, 1)
})

// The whole set arrives under one licence, so a licence we cannot ship stops all of it.
test('a baked set under a licence we may not ship is refused', () => {
  const { violations } = judgeLangConfig({ ...MANIFEST, licence: 'GPL-3.0' }, ['rust.json'], ALLOW)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /GPL-3\.0/)
})

// A licence read at a branch is a licence that has already moved by the time anybody checks it.
test('a manifest with no full revision is refused', () => {
  const { violations } = judgeLangConfig({ ...MANIFEST, revision: 'main' }, ['rust.json'], ALLOW)
  assert.ok(violations.some((v) => /no full revision/.test(v)))
})

test('a file in the tree that the manifest does not record is refused', () => {
  const { violations } = judgeLangConfig(MANIFEST, ['rust.json', 'perl.json'], ALLOW)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /perl\.json is in the tree but not in SOURCE\.json/)
})

test('a file the manifest records but the tree does not hold is refused', () => {
  const { violations } = judgeLangConfig(MANIFEST, [], ALLOW)
  assert.ok(violations.some((v) => /rust\.json is in SOURCE\.json but not in the tree/.test(v)))
})

test('a file recorded against some other revision than the pinned one is refused', () => {
  const files = [{ lang: 'rust', source: 'https://example.invalid/main/rust' }]
  const { violations } = judgeLangConfig({ ...MANIFEST, files }, ['rust.json'], ALLOW)
  assert.ok(violations.some((v) => /not recorded against the pinned revision/.test(v)))
})

// --- the baked grammars -------------------------------------------------------------------------

// The second baked tree, judged by the same function and named by the scope its files answer to
// rather than by a language. A grammar that arrives outside npm is one no dependency gate sees, so
// the manifest is the only place its licence was ever read.
const BAKED = {
  licence: 'MIT',
  repository: 'https://github.com/microsoft/vscode',
  revision: 'b'.repeat(40),
  files: [{ scope: 'text.html.php', source: `https://example.invalid/${'b'.repeat(40)}/php` }],
}

test('a baked grammar set that matches its manifest passes', () => {
  const { violations, judged } = judgeTmGrammar(BAKED, ['text.html.php.json'], ALLOW)
  assert.deepEqual(violations, [])
  assert.equal(judged, 1)
})

// The file is named after the scope, so a manifest naming the language instead would leave the tree
// and the record disagreeing about every file in it — which is the case the agreement check exists
// for, and the one a shared judgment could get wrong by reading the other tree's field.
test('a baked grammar recorded under any other name than its scope is refused', () => {
  const files = [{ lang: 'php', source: `https://example.invalid/${'b'.repeat(40)}/php` }]
  const { violations } = judgeTmGrammar({ ...BAKED, files }, ['text.html.php.json'], ALLOW)
  assert.ok(violations.some((v) => /text\.html\.php\.json is in the tree but not in SOURCE\.json/.test(v)))
})

test('a baked grammar under a licence we may not ship is refused', () => {
  const { violations } = judgeTmGrammar({ ...BAKED, licence: 'GPL-3.0' }, ['text.html.php.json'], ALLOW)
  assert.ok(violations.some((v) => /tmgrammar: GPL-3\.0/.test(v)))
})

test('the catalog we actually ship passes', async () => {
  const { violations } = judgeGrammars(
    bundledGrammars(await import('node:fs').then((fs) => fs.readFileSync('app/src/files/grammars.ts', 'utf8'))),
    new Set(['MIT']),
  )
  assert.deepEqual(violations, [])
})
