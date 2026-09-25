#!/usr/bin/env node
// Bake the notification wording out of the GUI's dictionaries and into the core.
//
// A notification's line is written in the language the reader chose, and the place that language is
// already written out nineteen times is `app/src/core/i18n/locales`. Core sends the notifications,
// and core cannot read a TypeScript module — so the `notify.*` keys are copied into a Rust table
// here, once, rather than kept as a second dictionary that would drift from the first.
//
// The drift is the whole reason this is a generator and not a hand-written table. What the plugins
// this replaced each carried was a dictionary of their own: two copies of nineteen languages, saying
// the same thing in two places, either of which could be corrected without the other. One source and
// a copy that is checked against it (`guards/check-notify-wording-fresh.sh`) is what keeps them one.
//
// What is written is tracked, the way the brand images and the language configurations are: a build
// reads files rather than build steps, and nothing in a merge waits on node. Run it when a `notify.*`
// key or a status label moves, and commit what changes.
//
//     make notify-wording
//
// Two sections are read and no more. `notify.*` out of `ui` is what a line says, and `status` is what
// Amenbo calls a state — taken from the dictionary rather than worded again, so a channel names a
// state the same way the app the reader would go and look it up in does.
//
// `auto.say.*` rides along for the same reason a notification does: it is the line a stopped run
// leaves on its task, which core writes and keeps as text, so the GUI cannot word it
// again when it is shown.
import { readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const locales = join(root, "app/src/core/i18n/locales");
const out = join(root, "crates/amenbo-core/src/notify_wording_table.rs");

/**
 * The family a sentence is written under, which the Rust table drops.
 *
 * It is `notify.say.` and not `notify.`: the notification screens are written under that shorter one
 * too, and a line in a channel and a button on a settings screen are not the same kind of string.
 * Reading the whole family would bake every label the screen draws into core, where nothing says them.
 */
const PREFIX = "notify.say.";

/** The family the line a stopped run leaves on its task is written under — its own, beside the run's labels. */
const RUN_PREFIX = "auto.say.";

/** The keys of one family out of `ui`, with the family dropped and sorted by what is left. */
function family(ui, prefix) {
  return Object.entries(ui ?? {})
    .filter(([key]) => key.startsWith(prefix))
    .map(([key, value]) => [key.slice(prefix.length), value])
    .sort(([a], [b]) => (a < b ? -1 : 1));
}

/**
 * A Rust string literal. Only the two characters that could end one are escaped; a control character
 * is refused rather than escaped, because a line of prose with one in it is a mistake in the
 * dictionary and not something to carry through.
 */
function rust(value) {
  if (/[\u0000-\u001f]/.test(value)) throw new Error(`a control character in: ${value}`);
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

function pairs(entries, indent) {
  return entries.map(([key, value]) => `${indent}(${rust(key)}, ${rust(value)}),`).join("\n");
}

const files = readdirSync(locales).filter((name) => name.endsWith(".ts")).sort();
const rows = [];
for (const file of files) {
  const language = file.slice(0, -".ts".length);
  const module = await import(join(locales, file));
  const dictionary = module[language] ?? Object.values(module)[0];
  const says = family(dictionary.ui, PREFIX);
  const runs = family(dictionary.ui, RUN_PREFIX);
  const statuses = Object.entries(dictionary.status ?? {}).sort(([a], [b]) => (a < b ? -1 : 1));
  if (says.length === 0) continue;
  rows.push({ language, says, runs, statuses });
}

const body = rows
  .map(
    (row) => `    Wording {
        language: ${rust(row.language)},
        says: &[
${pairs(row.says, "            ")}
        ],
        runs: &[
${pairs(row.runs, "            ")}
        ],
        statuses: &[
${pairs(row.statuses, "            ")}
        ],
    },`,
  )
  .join("\n");

writeFileSync(
  out,
  `//! **What a notification says, in the language the reader chose** — generated, do not edit.
//!
//! Written by \`scripts/gen-notify-wording.mjs\` out of \`app/src/core/i18n/locales\`, which is where
//! the nineteen languages are already kept. Editing this file instead of the dictionary puts the two
//! out of step, and \`guards/check-notify-wording-fresh.sh\` is what notices.
//!
//! Run \`make notify-wording\` after moving a \`notify.*\` or \`auto.say.*\` key or a status label, and commit what moves.

/// One language's side of a notification: a sentence per key, and Amenbo's own word for each state —
/// and the sentences of the line a stopped run leaves on its task ([\`crate::run_wording\`]).
///
/// All three are sorted by key, and a language that has not translated a key simply has no pair for it —
/// [\`crate::notify_wording\`] falls back to English one key at a time, so a half-written dictionary
/// costs that one sentence rather than the whole language.
pub(crate) struct Wording {
    pub(crate) language: &'static str,
    pub(crate) says: &'static [(&'static str, &'static str)],
    pub(crate) runs: &'static [(&'static str, &'static str)],
    pub(crate) statuses: &'static [(&'static str, &'static str)],
}

/// Every language a line can be written in, in the order the dictionaries are named.
pub(crate) const WORDINGS: &[Wording] = &[
${body}
];
`,
);
console.log(`-> ${out} (${rows.length} languages, ${rows[0].says.length} keys)`);
