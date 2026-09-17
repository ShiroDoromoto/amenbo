#!/usr/bin/env node
// check-grammar-licenses.mjs — the gate that reads what is inside the packages, and what was baked
// into the tree from outside npm altogether.
//
// Its sibling, check-npm-licenses.mjs, reads what npm declares: one license field per package. That
// is the right question for code, where a package is written by the people who publish it. It is
// the wrong question for `tm-grammars`, which declares itself MIT and then ships 260 TextMate
// grammars collected from as many different projects — some of them GPL-3.0, a good many asserting
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
// The editor's other borrowed data is judged here for the same reason and by the same allow-list.
// Two trees of it are VS Code's, baked in by a script rather than installed: the language
// configurations under `app/src/files/langconfig/`, and the grammars under
// `app/src/files/tmgrammar/` that no package republishes. Neither is an npm package at all, so no
// dependency gate has ever had a chance to see either.
//
// Like the npm gate, the verdict is a pure function of files in the tree — no install, no network.
// A grammar added to the panel without a line here goes red, which is the whole point: the line is
// where somebody has to have looked.
//
// Usage: node guards/check-grammar-licenses.mjs   (from the repo root)

import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { allowed, parse, parseAllowList, tokenize } from './check-npm-licenses.mjs'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const CATALOG = join(ROOT, 'app', 'src', 'files', 'grammars.ts')
const LANGCONFIG = join(ROOT, 'app', 'src', 'files', 'langconfig')
const TMGRAMMAR = join(ROOT, 'app', 'src', 'files', 'tmgrammar')

// What each baked tree is called in a violation, what its files are named after, and how to write it
// again. The judgment below is one function because the question is one question: the manifest says
// what was read and where, and the directory has to be exactly that.
const LANG_CONFIG_TREE = { dir: 'langconfig', key: 'lang', regen: 'make lang-config' }
const TM_GRAMMAR_TREE = { dir: 'tmgrammar', key: 'scope', regen: 'make tm-grammar' }

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
      'undistributable. The same README license covers toml.tmbundle and html.tmbundle verbatim.',
  },
}

// Every grammar the panel ships, by the name `tm-grammars` publishes it under. Each name is one
// grammar and one file: what a language is drawn into is chosen and named beside the language
// in the catalog, so `html`, `javascript` and `css` are three entries here rather than one carrying
// the other two.
//
// `license` is an SPDX expression judged against deny.toml's allow-list; `grant` names an entry
// above instead, for a source that states terms without an identifier. Exactly one of the two.
// `source` is the revision the license was read at — `tm-grammars` publishes the same revisions the
// set this replaces did, so moving between them read nothing back.
const GRAMMARS = {
  css: [
    { grammar: 'css', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/af600487b1e94374d9f48f57cbf2cad24656b07f/extensions/css/syntaxes/css.tmLanguage.json' },
  ],
  go: [
    { grammar: 'go', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/091ef378baaa141c8bc4bbe9775d4cb3bd655a80/extensions/go/syntaxes/go.tmLanguage.json' },
  ],
  html: [
    { grammar: 'html', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/45324363153075dab0482312ae24d8c068d81e4f/extensions/html/syntaxes/html.tmLanguage.json' },
  ],
  javascript: [
    { grammar: 'javascript', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/210541906e5a96ab39f9c753f921b1bd35f4138b/extensions/javascript/syntaxes/JavaScript.tmLanguage.json' },
  ],
  json: [
    { grammar: 'json', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/d6af4893ed9a3545163a4cb748fa5548bd1e51a5/extensions/json/syntaxes/JSON.tmLanguage.json' },
  ],
  markdown: [
    { grammar: 'markdown', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/6d8ab9737d58fc5eaf07e2ae6553b38183a5de47/extensions/markdown-basics/syntaxes/markdown.tmLanguage.json' },
  ],
  php: [
    { grammar: 'php', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/af600487b1e94374d9f48f57cbf2cad24656b07f/extensions/php/syntaxes/php.tmLanguage.json' },
  ],
  // Converted out of textmate/html.tmbundle, which is the grant below and not VS Code's own licence
  // — the file says where it came from, and the bundle's README is where the terms are.
  'html-derivative': [
    { grammar: 'html-derivative', grant: 'textmate-bundle', source: 'https://github.com/textmate/html.tmbundle/blob/390c8870273a2ae80244dae6db6ba064a802f407/Syntaxes/HTML%20(Derivative).tmLanguage' },
  ],
  java: [
    { grammar: 'java', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/3c86ede5f554f6e196c832394e126b291a1de606/extensions/java/syntaxes/java.tmLanguage.json' },
  ],
  xml: [
    { grammar: 'xml', license: 'MIT', source: 'https://github.com/microsoft/vscode/blob/10a1d2a50a2882f5ae85bdb51eb04d3064fb9de9/extensions/xml/syntaxes/xml.tmLanguage.json' },
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
// `tm-grammars/grammars/<id>.json` specifier in it. Reading the specifiers rather than the map's
// keys is deliberate: the specifier is what the bundler acts on, so it is what actually ships — and
// now that a language names the grammars it is drawn into, the keys are not even the same set.
export function bundledGrammars(source) {
  const ids = [...source.matchAll(/["']tm-grammars\/grammars\/([\w.-]+)\.json["']/g)].map((m) => m[1])
  if (ids.length === 0) {
    refuse(`${CATALOG} imports no tm-grammars grammar — the catalog moved; fix this gate`)
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

// --- the baked trees ----------------------------------------------------------------------------

// What the manifest a generator writes has to say before the files beside it can be trusted: one
// licence for the lot, a revision anybody can go back and re-read it at, and a per-file record.
//
// `tree` is which of the baked trees this is — the name to say it by, the field its files are named
// after, and the command that writes it again. Both trees are judged by this one function because
// they are one question asked twice: a tree whose manifest and directory disagree is a tree where
// somebody put a file in by hand, which is exactly the case where nobody read a licence.
export function judgeBaked(manifest, present, allow, tree) {
  const { dir, key, regen } = tree
  const violations = []

  if (typeof manifest?.licence !== 'string') {
    violations.push(`${dir}/SOURCE.json records no licence — re-run \`${regen}\``)
    return { violations, judged: 0 }
  }
  let ok
  try {
    ok = allowed(parse(tokenize(manifest.licence)), allow)
  } catch (e) {
    violations.push(`${dir}/SOURCE.json: cannot read the licence "${manifest.licence}" (${e.message})`)
    return { violations, judged: 0 }
  }
  if (!ok) violations.push(`${dir}: ${manifest.licence} (${manifest.repository}) is not allowed`)
  if (typeof manifest.revision !== 'string' || !/^[0-9a-f]{40}$/.test(manifest.revision)) {
    violations.push(`${dir}/SOURCE.json records no full revision — a licence read at a moving branch is one nobody can re-read`)
  }

  // The manifest and the directory have to agree. A file put there by hand, or one the generator
  // dropped, is exactly the case where nobody read anything.
  const recorded = new Set((manifest.files ?? []).map((f) => `${f[key]}.json`))
  for (const f of manifest.files ?? []) {
    if (typeof f.source !== 'string' || !f.source.includes(manifest.revision)) {
      violations.push(`${dir}: ${f[key]} is not recorded against the pinned revision`)
    }
  }
  for (const file of present) {
    if (!recorded.has(file)) violations.push(`${dir}/${file} is in the tree but not in SOURCE.json`)
  }
  for (const file of recorded) {
    if (!present.includes(file)) violations.push(`${dir}/${file} is in SOURCE.json but not in the tree`)
  }

  return { violations, judged: recorded.size }
}

/** The language configurations, which are named by the language they configure. */
export const judgeLangConfig = (manifest, present, allow) =>
  judgeBaked(manifest, present, allow, LANG_CONFIG_TREE)

/** The grammars no package republishes, which are named by the scope they answer to. */
export const judgeTmGrammar = (manifest, present, allow) =>
  judgeBaked(manifest, present, allow, TM_GRAMMAR_TREE)

// --- the gate ----------------------------------------------------------------------------------

/** A count of files, said the way a count of one is said. */
const files = (n) => `${n} ${n === 1 ? 'file' : 'files'}`

function main() {
  const allow = parseAllowList(readFileSync(join(ROOT, 'deny.toml'), 'utf8'))
  const bundled = bundledGrammars(readFileSync(CATALOG, 'utf8'))
  const { violations, judged, usedGrants } = judgeGrammars(bundled, allow)

  const manifest = JSON.parse(readFileSync(join(LANGCONFIG, 'SOURCE.json'), 'utf8'))
  const present = readdirSync(LANGCONFIG).filter((f) => f.endsWith('.json') && f !== 'SOURCE.json')
  const config = judgeLangConfig(manifest, present, allow)
  violations.push(...config.violations)

  const baked = JSON.parse(readFileSync(join(TMGRAMMAR, 'SOURCE.json'), 'utf8'))
  const there = readdirSync(TMGRAMMAR).filter((f) => f.endsWith('.json') && f !== 'SOURCE.json')
  const grammars = judgeTmGrammar(baked, there, allow)
  violations.push(...grammars.violations)

  if (violations.length > 0) {
    console.error('✗ grammar license gate:')
    for (const v of violations) console.error(`    ${v}`)
    console.error('  Data amenbo ships is bound by the same allow-list as everything else:')
    console.error("  deny.toml's [licenses] allow. Read it at its source and record the verdict with that URL")
    console.error('  — in GRAMMARS here, or by re-running `make lang-config` or `make tm-grammar` — or take the data back out.')
    return 1
  }

  console.log(`→ grammar licenses: ${judged.size} grammars named by the panel's catalog, all within deny.toml's allow-list`)
  console.log(`→ language configurations: ${files(config.judged)}, ${manifest.licence} from ${manifest.repository} at ${manifest.tag}`)
  console.log(`→ baked grammars: ${files(grammars.judged)}, ${baked.licence} from ${baked.repository} at ${baked.tag}`)
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
