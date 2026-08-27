#!/usr/bin/env node
// Bake VS Code's language configurations into the file panel's editor.
//
// A TextMate grammar says what a run of characters IS. It does not say what happens when somebody
// presses Enter after `{`, which bracket the one under the caret answers to, or where a block ends.
// VS Code keeps those in a second file per language — `language-configuration.json` — and writes
// them itself: the `cgmanifest.json` beside each built-in extension registers the grammars it
// vendored from elsewhere, and never the configuration, so the configuration is Microsoft's own
// work under the repository's MIT licence.
//
// Nobody running Amenbo has VS Code, so the data is baked in here rather than read from a machine.
// What is written is tracked, for the same reason the brand images are (`gen-brand.py`): bundlers
// read files, not build steps, and a merge should not wait on a network fetch. Re-run it by hand
// when the pinned revision moves, and commit what changes.
//
//     make lang-config
//
// Three things happen on the way in, and all three are why this is a build step rather than a
// runtime read:
//
//   1. **The files are not JSON.** Comments and trailing commas — JSONC — and 21 of the 56 VS Code
//      ships fail `JSON.parse` outright. Converting once here means no parser rides into the
//      window.
//   2. **Only what the editor reads is kept.** The rest (word patterns, colorized bracket pairs,
//      auto-closing-before sets) is dropped, so a reader of the tracked file sees exactly what has
//      an effect.
//   3. **One shipped pattern is broken and is repaired.** See `REPAIRS`.
//
// Needs network access. Nothing here runs in CI.

import { createHash } from 'node:crypto'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const OUT = join(ROOT, 'app', 'src', 'files', 'langconfig')

// The revision every file below is read at. A tag rather than a branch: what is tracked here has to
// be reproducible from one name, and `main` is not one.
const VSCODE_TAG = '1.135.0'
const VSCODE_REV = '08d4889f9ec4a1685d257b9b95de036c8e1ce1e5'

// Which built-in extension holds each language's configuration. The keys are the language ids in
// `app/src/files/grammars.ts` — a language with no entry here gets the generic bracket rules, which
// are already worth 85% of the indentation in a brace language on their own.
//
// `toml` has no entry because VS Code has no built-in TOML extension. `tsx` takes typescript-basics
// for the same reason its grammar is the TypeScriptReact one: it is the configuration for the whole
// JavaScript family.
const EXTENSIONS = {
  css: 'css',
  go: 'go',
  html: 'html',
  json: 'json',
  markdown: 'markdown-basics',
  python: 'python',
  rust: 'rust',
  shellscript: 'shellscript',
  sql: 'sql',
  tsx: 'typescript-basics',
  yaml: 'yaml',
}

// A pattern that is wrong where it is published. Repairing it here rather than in the editor keeps
// the fix next to the evidence: the tracked file says what we ship, and this says why it differs
// from upstream.
//
// Each entry is checked: if the text to replace is no longer there, the repair is stale and the
// bake stops rather than silently shipping an assumption nobody re-read.
const REPAIRS = [
  {
    lang: 'yaml',
    at: 'indentationRules.increaseIndentPattern',
    // The rule means to allow a YAML anchor (`&name`) after the colon. What is published has the
    // ampersand HTML-escaped, so it matches the literal text `&amp;name` and never an anchor.
    from: '(&amp;\\w+)?',
    to: '(&\\w+)?',
  },
]

// What the editor reads. Everything else in the file is dropped rather than carried, so the tracked
// output is exactly the effective configuration.
const KEEP = [
  'comments',
  'brackets',
  'autoClosingPairs',
  'surroundingPairs',
  'indentationRules',
  'folding',
  'onEnterRules',
]

/** Strip JSONC down to JSON: line and block comments, and trailing commas. */
export function fromJsonc(text) {
  let out = ''
  let i = 0
  while (i < text.length) {
    const c = text[i]
    if (c === '"') {
      // A string is copied whole, so a `//` or a `,` inside one is never read as syntax.
      let j = i + 1
      while (j < text.length && text[j] !== '"') j += text[j] === '\\' ? 2 : 1
      out += text.slice(i, j + 1)
      i = j + 1
      continue
    }
    if (c === '/' && text[i + 1] === '/') {
      const end = text.indexOf('\n', i)
      i = end === -1 ? text.length : end
      continue
    }
    if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2)
      i = end === -1 ? text.length : end + 2
      continue
    }
    out += c
    i += 1
  }
  // A comma with only whitespace between it and the closing brace or bracket.
  return out.replace(/,(\s*[}\]])/g, '$1')
}

const url = (ext) =>
  `https://raw.githubusercontent.com/microsoft/vscode/${VSCODE_REV}/extensions/${ext}/language-configuration.json`

/** Apply the repairs that name `lang`, and refuse to pass one whose target has moved. */
export function repair(lang, config, repairs = REPAIRS) {
  for (const r of repairs.filter((one) => one.lang === lang)) {
    const path = r.at.split('.')
    const holder = path.slice(0, -1).reduce((o, k) => o?.[k], config)
    const key = path[path.length - 1]
    const before = holder?.[key]
    if (typeof before !== 'string' || !before.includes(r.from)) {
      throw new Error(
        `${lang}: the repair of ${r.at} no longer applies — upstream has changed it. ` +
          `Re-read it at ${VSCODE_TAG} and either drop the repair or write a new one.`,
      )
    }
    holder[key] = before.replace(r.from, r.to)
  }
  return config
}

async function bake() {
  mkdirSync(OUT, { recursive: true })
  const provenance = []

  for (const [lang, ext] of Object.entries(EXTENSIONS)) {
    const response = await fetch(url(ext))
    if (!response.ok) throw new Error(`${lang}: ${url(ext)} answered ${response.status}`)
    const raw = await response.text()

    const parsed = JSON.parse(fromJsonc(raw))
    const kept = Object.fromEntries(KEEP.filter((k) => k in parsed).map((k) => [k, parsed[k]]))
    const config = repair(lang, kept)

    writeFileSync(join(OUT, `${lang}.json`), `${JSON.stringify(config, null, 2)}\n`)
    provenance.push({
      lang,
      source: url(ext),
      sha256: createHash('sha256').update(raw).digest('hex'),
      repaired: REPAIRS.filter((r) => r.lang === lang).map((r) => r.at),
    })
    console.log(`→ ${lang}.json  (from extensions/${ext})`)
  }

  writeFileSync(
    join(OUT, 'SOURCE.json'),
    `${JSON.stringify(
      {
        note: 'Written by scripts/gen-lang-config.mjs. Do not edit by hand; re-run `make lang-config`.',
        repository: 'https://github.com/microsoft/vscode',
        licence: 'MIT',
        tag: VSCODE_TAG,
        revision: VSCODE_REV,
        files: provenance,
      },
      null,
      2,
    )}\n`,
  )
  console.log(`→ SOURCE.json   (microsoft/vscode ${VSCODE_TAG}, MIT)`)
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  await bake()
}
