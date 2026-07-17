// Tests for the npm license gate.
//
// The gate runs on every dependency bump, but a real run only ever exercises the tree we HAVE —
// which is green. So a run proves the gate can say yes; nothing proves it can still say no. An
// evaluator that returned `true` unconditionally would sail through CI for years, silently allowing
// the one thing it exists to stop. These tests are therefore mostly about the NO: each case below
// is a way the gate must refuse.
//
// Run: node --test guards/
//
// The gate reads deny.toml and package-lock.json only inside main(); everything asserted here is
// pure, so the cases are synthetic and no real file is touched.

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { allowed, judge, parse, parseAllowList, shippedPackages, tokenize } from './check-npm-licenses.mjs'

const ALLOW = new Set(['MIT', 'Apache-2.0', 'ISC', 'BSD-3-Clause', 'MPL-2.0', 'CC0-1.0', 'Apache-2.0 WITH LLVM-exception'])
const verdict = (expr) => allowed(parse(tokenize(expr)), ALLOW)
const pkgs = (list) => new Map(list.map((p) => [`node_modules/${p.name}`, p]))

// --- SPDX expressions --------------------------------------------------------------------------

test('a bare id is judged against the allow-list', () => {
  assert.equal(verdict('MIT'), true)
  assert.equal(verdict('GPL-3.0'), false)
  assert.equal(verdict('AGPL-3.0-only'), false)
})

test('OR is a choice: we may take the side we can live with', () => {
  assert.equal(verdict('MIT OR Apache-2.0'), true)
  assert.equal(verdict('Apache-2.0 OR MIT'), true)
  assert.equal(verdict('(MPL-2.0 OR Apache-2.0)'), true, 'parenthesised, as npm writes dompurify')
  assert.equal(verdict('MIT OR GPL-3.0'), true, 'a real dual license: we take the MIT side')
  assert.equal(verdict('GPL-3.0 OR AGPL-3.0'), false, 'no side is one we can ship')
})

// The trap a substring match falls into, and the reason this is parsed at all.
test('AND is a conjunction: copyleft cannot hide behind a permissive half', () => {
  assert.equal(verdict('MIT AND GPL-3.0'), false)
  assert.equal(verdict('GPL-3.0 AND MIT'), false)
  assert.equal(verdict('MIT AND CC0-1.0'), true, 'both halves are allowed')
})

test('parentheses bind tighter than the operators around them', () => {
  assert.equal(verdict('(MIT OR GPL-3.0) AND ISC'), true)
  assert.equal(verdict('(GPL-3.0 OR AGPL-3.0) AND MIT'), false)
  assert.equal(verdict('MIT AND (GPL-3.0 OR ISC)'), true)
})

test('AND binds tighter than OR, so a GPL conjunction does not poison the whole choice', () => {
  // "MIT OR (Apache-2.0 AND GPL-3.0)" — the MIT side stands on its own.
  assert.equal(verdict('MIT OR Apache-2.0 AND GPL-3.0'), true)
  // "(GPL-3.0 AND MIT) OR (GPL-3.0 AND ISC)" — neither side is takeable.
  assert.equal(verdict('GPL-3.0 AND MIT OR GPL-3.0 AND ISC'), false)
})

test('WITH belongs to the id it follows, and a + means that version or later', () => {
  assert.equal(verdict('Apache-2.0 WITH LLVM-exception'), true)
  assert.equal(verdict('GPL-3.0 WITH Classpath-exception-2.0'), false, 'the exception does not make it shippable')
  assert.equal(verdict('Apache-2.0+'), true)
})

test('an unreadable expression throws rather than passing', () => {
  assert.throws(() => verdict('(MIT'), /unbalanced parentheses/)
  assert.throws(() => verdict('MIT OR'), /unexpected end/)
  assert.throws(() => verdict('MIT Apache-2.0'), /trailing tokens/)
  // npm's escape hatch for a bespoke license. It is not an SPDX expression and must not be guessed at.
  assert.throws(() => verdict('SEE LICENSE IN LICENSE.txt'))
})

// --- the allow-list, out of deny.toml ----------------------------------------------------------

test('the allow-list is read from deny.toml [licenses]', () => {
  const allow = parseAllowList('[licenses]\nallow = [\n  "MIT",\n  "Apache-2.0",\n]\n')
  assert.deepEqual([...allow].sort(), ['Apache-2.0', 'MIT'])
})

test('a commented-out id is not allowed just because it is written there', () => {
  const allow = parseAllowList('[licenses]\nallow = [\n  "MIT",\n  # never: "GPL-3.0" would make the binaries undistributable\n]\n')
  assert.deepEqual([...allow], ['MIT'])
})

test('the list stops at the next table, so a later section cannot widen it', () => {
  const allow = parseAllowList('[licenses]\nallow = [\n  "MIT",\n]\n\n[bans]\nallow = [\n  "GPL-3.0",\n]\n')
  assert.deepEqual([...allow], ['MIT'])
})

// If the allow-list ever moves, the gate must stop and say so. Yielding an empty set would fail
// every package (a red nobody trusts), and a partial set would quietly let something through.
test('a deny.toml it cannot read stops the gate instead of guessing', () => {
  assert.throws(() => parseAllowList('[bans]\nmultiple-versions = "warn"\n'), /no \[licenses\] section/)
  assert.throws(() => parseAllowList('[licenses]\nunlicensed = "deny"\n'), /no allow = /)
  assert.throws(() => parseAllowList('[licenses]\nallow = [\n]\n'), /parsed as empty/)
})

// --- reading the lockfile ----------------------------------------------------------------------

test('only what ships is judged', () => {
  const found = shippedPackages({
    lockfileVersion: 3,
    packages: {
      '': { name: 'amenbo-app', license: 'Apache-2.0' },
      'node_modules/react': { version: '18.3.1', license: 'MIT' },
      'node_modules/vitest': { version: '4.1.10', license: 'MIT', dev: true },
      'node_modules/@types/node': { version: '24.0.0', license: 'MIT', devOptional: true },
      'node_modules/fsevents': { version: '2.3.3', license: 'MIT', optional: true },
      'node_modules/some-workspace': { link: true, resolved: 'packages/x' },
    },
  })
  assert.deepEqual(
    [...found.values()].map((p) => p.name),
    ['react', 'fsevents'],
    'dev and devOptional do not ship; the root is ours; a link is judged where it lives; a prod optional CAN ship',
  )
})

test('a nested package keeps its own name', () => {
  const found = shippedPackages({
    lockfileVersion: 3,
    packages: { 'node_modules/a/node_modules/b': { version: '1.0.0', license: 'MIT' } },
  })
  assert.equal([...found.values()][0].name, 'b')
})

test('a lockfile too old to carry licenses is refused, not read as clean', () => {
  assert.throws(() => shippedPackages({ lockfileVersion: 1, dependencies: {} }), /needs 3\+/)
  assert.throws(() => shippedPackages({ packages: {} }), /needs 3\+/, 'no version at all')
})

test('an empty shipped set is refused rather than passed vacuously', () => {
  assert.throws(() => shippedPackages({ lockfileVersion: 3, packages: {} }), /no shipped dependencies/)
})

// --- the verdict -------------------------------------------------------------------------------

const NONE = {}

test('a tree of allowed licenses passes', () => {
  const { violations, undeclared } = judge(pkgs([{ name: 'react', version: '18.3.1', license: 'MIT' }]), ALLOW, NONE)
  assert.deepEqual(violations, [])
  assert.deepEqual(undeclared, [])
})

test('a copyleft package arriving in the shipped tree fails the gate', () => {
  const { violations } = judge(pkgs([{ name: 'evil', version: '1.0.0', license: 'GPL-3.0' }]), ALLOW, NONE)
  assert.equal(violations.length, 1)
  assert.match(violations[0], /evil@1\.0\.0: GPL-3\.0 is not allowed/)
})

test('a package that declares nothing is not guessed at', () => {
  const { undeclared, violations } = judge(pkgs([{ name: 'mystery', version: '1.0.0', license: undefined }]), ALLOW, NONE)
  assert.equal(violations.length, 0)
  assert.match(undeclared[0], /mystery@1\.0\.0 declares no license/)
})

test('an unparseable expression is reported, not shrugged off', () => {
  const { violations } = judge(pkgs([{ name: 'odd', version: '1.0.0', license: 'SEE LICENSE IN LICENSE.txt' }]), ALLOW, NONE)
  assert.match(violations[0], /cannot read the license expression/)
})

// --- the exceptions, and their other half ------------------------------------------------------

const KHROMA = { khroma: { license: 'MIT', licenseFile: 'license', why: 'test fixture' } }

test('an exception covers a package that declares no license', () => {
  const { violations, undeclared, usedExceptions } = judge(
    pkgs([{ name: 'khroma', version: '2.1.0', license: undefined }]),
    ALLOW,
    KHROMA,
  )
  assert.deepEqual(violations, [])
  assert.deepEqual(undeclared, [])
  assert.ok(usedExceptions.has('khroma'))
})

test('an exception cannot smuggle in a license we do not allow', () => {
  const exceptions = { bad: { license: 'GPL-3.0', licenseFile: 'LICENSE', why: 'test fixture' } }
  const { violations } = judge(pkgs([{ name: 'bad', version: '1.0.0', license: undefined }]), ALLOW, exceptions)
  assert.match(violations[0], /GPL-3\.0 \(read from its LICENSE\) is not allowed/)
})

test('an exception whose package now declares a license is called out', () => {
  const { violations } = judge(pkgs([{ name: 'khroma', version: '2.2.0', license: 'MIT' }]), ALLOW, KHROMA)
  assert.match(violations[0], /now declares "MIT" itself — drop the entry/)
})

test('an exception for a package that no longer ships is called out', () => {
  const { violations } = judge(pkgs([{ name: 'react', version: '18.3.1', license: 'MIT' }]), ALLOW, KHROMA)
  assert.match(violations[0], /khroma is exempted here but is no longer a shipped dependency/)
})
