#!/usr/bin/env node
// Bake the TextMate grammars that no package publishes into the file panel's editor.
//
// Nearly every grammar the panel colours with comes from `tm-grammars`, one name at a time
// (`app/src/files/grammars.ts`). This is for the ones that package does not carry: a grammar VS Code
// ships inside an extension and nobody has republished. There is one of them, and it is the root a
// PHP file is read from — `text.html.php`, which reads the HTML a PHP file is written inside and
// hands the parts between `<?php` and `?>` to `source.php`.
//
// Nobody running Amenbo has VS Code, so the data is baked in here rather than read from a machine —
// the same shape, and for the same reasons, as the language configurations beside it
// (`gen-lang-config.mjs`): what is written is tracked, because bundlers read files rather than
// build steps and a merge should not wait on a network fetch. Re-run it by hand when the pinned
// revision moves, and commit what changes.
//
//     make tm-grammar
//
// **What is baked is judged like everything else we ship.** `guards/check-grammar-licenses.mjs`
// reads `SOURCE.json` written here — the licence, the revision it was read at, and one line per
// file — because a grammar that arrives outside npm is one no dependency gate would ever see.
//
// Needs network access. Nothing here runs in CI.

import { createHash } from 'node:crypto'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const OUT = join(ROOT, 'app', 'src', 'files', 'tmgrammar')

// The revision the files below are read at. A tag rather than a branch, for the reason the language
// configurations pin one: what is tracked here has to be reproducible from one name.
const VSCODE_TAG = '1.135.0'
const VSCODE_REV = '08d4889f9ec4a1685d257b9b95de036c8e1ce1e5'

// Every grammar baked from VS Code, by the scope it answers to — which is the name a registry asks
// for it under, and so the name the file is written as.
//
// `text.html.php` is the whole list on purpose. A grammar `tm-grammars` publishes belongs in the
// catalog, where the gate reads it by its package name; this is only for what that package has not
// got.
const GRAMMARS = {
  'text.html.php': 'php/syntaxes/html.tmLanguage.json',
}

const url = (path) =>
  `https://raw.githubusercontent.com/microsoft/vscode/${VSCODE_REV}/extensions/${path}`

async function bake() {
  mkdirSync(OUT, { recursive: true })
  const provenance = []

  for (const [scope, path] of Object.entries(GRAMMARS)) {
    const response = await fetch(url(path))
    if (!response.ok) throw new Error(`${scope}: ${url(path)} answered ${response.status}`)
    const raw = await response.text()

    // A grammar is kept whole — every pattern in it is read by the tokenizer, so there is nothing
    // to drop the way a language configuration has parts the editor never asks about.
    const parsed = JSON.parse(raw)
    if (parsed.scopeName !== scope) {
      throw new Error(`${path} answers to ${parsed.scopeName}, not ${scope} — the file moved`)
    }

    writeFileSync(join(OUT, `${scope}.json`), `${JSON.stringify(parsed, null, 2)}\n`)
    provenance.push({
      scope,
      source: url(path),
      sha256: createHash('sha256').update(raw).digest('hex'),
    })
    console.log(`→ ${scope}.json  (from extensions/${path})`)
  }

  writeFileSync(
    join(OUT, 'SOURCE.json'),
    `${JSON.stringify(
      {
        note: 'Written by scripts/gen-tm-grammar.mjs. Do not edit by hand; re-run `make tm-grammar`.',
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
