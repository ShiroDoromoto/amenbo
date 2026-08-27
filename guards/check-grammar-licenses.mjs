#!/usr/bin/env node
// check-grammar-licenses.mjs — the gate that reads what is inside the packages.
//
// Its sibling, check-npm-licenses.mjs, reads what npm declares: one license field per package. That
// is the right question for code, where a package is written by the people who publish it. It is
// the wrong question for `@shikijs/langs`, which declares itself MIT and then ships 361 TextMate
// grammars collected from as many different projects — five of them GPL-3.0, a dozen more asserting
// nothing at all. The package's own field says nothing about any of them, so a grammar riding into
// an Apache-2.0 bundle would pass the npm gate on a license it does not have.
//
// So the set is not the package. The set is what `app/src/files/grammars.ts` names, one grammar at
// a time, and this gate holds every name in it — plus every grammar those names drag in with them —
// to a license somebody read at the source, recorded below with the URL they read it from.
//
// The allow-list is NOT repeated here: it is deny.toml's, read through the npm gate, so all three
// license gates (cargo, npm, this) judge by the one policy.
//
// Like the npm gate, the verdict is a pure function of files in the tree — no install, no network.
// A grammar added to the panel without a line here goes red, which is the whole point: the line is
// where somebody has to have looked.
//
// Usage: node guards/check-grammar-licenses.mjs   (from the repo root)

import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { allowed, parse, parseAllowList, tokenize } from './check-npm-licenses.mjs'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const CATALOG = join(ROOT, 'app', 'src', 'files', 'grammars.ts')

// A license with no SPDX identifier to judge it by. The allow-list speaks SPDX and this text has no
// entry in it, so the grant itself is written out — a human read it at the URL below and the words
// are here to be re-read rather than trusted. Both entries are TextMate's own bundles, which
// predate SPDX and carry one README license for the whole repository.
//
// This is the only door around the allow-list, and it is deliberately narrow: a grant nothing
// claims goes red, exactly as a stale exception does in the npm gate.
const GRANTS = {
  'textmate-bundle': {
    from: 'https://github.com/textmate/yaml.tmbundle#license',
    text:
      'Permission to copy, use, modify, sell and distribute this software is granted. ' +
      'This software is provided "as is" without express or implied warranty, and with no ' +
      'claim as to its suitability for any purpose.',
    why: 'Grants copying, modification, sale and distribution with no condition attached — no ' +
      'notice to carry, no source to publish. Nothing in it can make an Apache-2.0 bundle ' +
      'undistributable. The same README license covers toml.tmbundle verbatim.',
  },
}

// Every grammar the panel ships, by the `@shikijs/langs` module it arrives in. A module carries the
// grammars it embeds as well as its own — `html` descends into `<script>` and `<style>`, so it
// brings JavaScript and CSS with it, and those are as shipped as anything named directly.
//
// `license` is an SPDX expression judged against deny.toml's allow-list; `grant` names an entry
// above instead, for a source that states terms without an identifier. Exactly one of the two.
// `source` is the revision the license was read at.
const GRAMMARS = {
  css: [
    { grammar: 'css', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/af600487b1e94374d9f48f57cbf2cad24656b07f/extensions/css/syntaxes/css.tmLanguage.json' },
  ],
  go: [
    { grammar: 'go', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/091ef378baaa141c8bc4bbe9775d4cb3bd655a80/extensions/go/syntaxes/go.tmLanguage.json' },
  ],
  html: [
    { grammar: 'html', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/45324363153075dab0482312ae24d8c068d81e4f/extensions/html/syntaxes/html.tmLanguage.json' },
    { grammar: 'javascript', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/210541906e5a96ab39f9c753f921b1bd35f4138b/extensions/javascript/syntaxes/JavaScript.tmLanguage.json' },
    { grammar: 'css', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/af600487b1e94374d9f48f57cbf2cad24656b07f/extensions/css/syntaxes/css.tmLanguage.json' },
  ],
  json: [
    { grammar: 'json', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/d6af4893ed9a3545163a4cb748fa5548bd1e51a5/extensions/json/syntaxes/JSON.tmLanguage.json' },
  ],
  markdown: [
    { grammar: 'markdown', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/6d8ab9737d58fc5eaf07e2ae6553b38183a5de47/extensions/markdown-basics/syntaxes/markdown.tmLanguage.json' },
  ],
  python: [
    { grammar: 'python', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/cf4c9e469d521fa5f33353737e8157eb0789ad02/extensions/python/syntaxes/MagicPython.tmLanguage.json' },
  ],
  rust: [
    { grammar: 'rust', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/af600487b1e94374d9f48f57cbf2cad24656b07f/extensions/rust/syntaxes/rust.tmLanguage.json' },
  ],
  shellscript: [
    { grammar: 'shellscript', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/9473445f7d3dcb5c579f42ece8b6c18c43c63ed3/extensions/shellscript/syntaxes/shell-unix-bash.tmLanguage.json' },
  ],
  sql: [
    { grammar: 'sql', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/af600487b1e94374d9f48f57cbf2cad24656b07f/extensions/sql/syntaxes/sql.tmLanguage.json' },
  ],
  toml: [
    { grammar: 'toml', grant: 'textmate-bundle', source: 'https://github.com/textmate/toml.tmbundle/blob/e82b64c1e86396220786846201e9aa3f0a2d9ca2/Syntaxes/TOML.tmLanguage' },
  ],
  tsx: [
    { grammar: 'tsx', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/210541906e5a96ab39f9c753f921b1bd35f4138b/extensions/typescript-basics/syntaxes/TypeScriptReact.tmLanguage.json' },
  ],
  yaml: [
    { grammar: 'yaml', grant: 'textmate-bundle', source: 'https://github.com/textmate/yaml.tmbundle/blob/e54ceae3b719506dba7e481a77cea4a8b576ae46/Syntaxes/YAML.tmLanguage' },
  ],
}

class GateError extends Error {}
const refuse = (msg) => {
  throw new GateError(msg)
}

// --- what the panel actually bundles -----------------------------------------------------------

// The catalog is TypeScript and node has no TypeScript parser, but the shape we need is every
// `@shikijs/langs/<id>` specifier in it. Reading the specifiers rather than the map's keys is
// deliberate: the specifier is what the bundler acts on, so it is what actually ships.
export function bundledGrammars(source) {
  const ids = [...source.matchAll(/["']@shikijs\/langs\/([\w.-]+)["']/g)].map((m) => m[1])
  if (ids.length === 0) {
    refuse(`${CATALOG} imports no @shikijs/langs grammar — the catalog moved; fix this gate`)
  }
  return new Set(ids)
}

// --- the judgment ------------------------------------------------------------------------------

// Pure: the bundled set + the allow-list + the two tables in, verdict out. Nothing read, nothing
// printed, nothing exited — which is what lets the tests drive it with a synthetic catalog and
// assert that the things that MUST go red actually do.
export function judgeGrammars(bundled, allow, table = GRAMMARS, grants = GRANTS) {
  const violations = []
  const judged = new Set()
  const usedGrants = new Set()

  for (const id of [...bundled].sort()) {
    const carried = table[id]
    if (carried === undefined) {
      violations.push(`${id} is bundled but no licence is recorded for it — read its source and add it here`)
      continue
    }
    for (const { grammar, license, grant, source } of carried) {
      judged.add(grammar)
      if (grant !== undefined) {
        const known = grants[grant]
        if (known === undefined) {
          violations.push(`${id}: ${grammar} names the grant "${grant}", which is not recorded here`)
          continue
        }
        usedGrants.add(grant)
        continue
      }
      let ok
      try {
        ok = allowed(parse(tokenize(license)), allow)
      } catch (e) {
        violations.push(`${id}: cannot read ${grammar}'s license expression "${license}" (${e.message})`)
        continue
      }
      if (!ok) violations.push(`${id}: ${grammar} is ${license} (${source}), which is not allowed`)
    }
  }

  // A grammar recorded here and no longer bundled is a licence nobody is relying on, and the
  // reader of this file would take it for one we ship. The npm gate drops its stale exceptions for
  // the same reason.
  for (const id of Object.keys(table)) {
    if (!bundled.has(id)) {
      violations.push(`${id} is recorded here but is no longer bundled by the panel — drop the entry`)
    }
  }
  for (const grant of Object.keys(grants)) {
    if (!usedGrants.has(grant)) {
      violations.push(`the grant "${grant}" is recorded here but nothing claims it — drop the entry`)
    }
  }

  return { violations, judged, usedGrants }
}

// --- the gate ----------------------------------------------------------------------------------

function main() {
  const allow = parseAllowList(readFileSync(join(ROOT, 'deny.toml'), 'utf8'))
  const bundled = bundledGrammars(readFileSync(CATALOG, 'utf8'))
  const { violations, judged, usedGrants } = judgeGrammars(bundled, allow)

  if (violations.length > 0) {
    console.error('✗ grammar license gate:')
    for (const v of violations) console.error(`    ${v}`)
    console.error('  A TextMate grammar is data amenbo ships, so it is bound by the same allow-list as')
    console.error("  everything else: deny.toml's [licenses] allow. Read the grammar at its source, record")
    console.error('  the verdict with that URL in GRAMMARS in this file, or take the grammar back out.')
    return 1
  }

  console.log(`→ grammar licenses: ${judged.size} grammars in ${bundled.size} bundled modules, all within deny.toml's allow-list`)
  for (const grant of usedGrants) {
    console.log(`  (${grant}: no SPDX identifier — the grant is quoted in this file, read from ${GRANTS[grant].from})`)
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
