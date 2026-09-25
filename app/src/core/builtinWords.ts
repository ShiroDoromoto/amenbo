// **A built-in's words, in the language the screen is drawn in** (`AMB-D-964`).
//
// A built-in is written into the store in Japanese — its name, what it does, its settings and their
// choices, its ways out and what it hands on — and those words stay as they are: the code that
// carries a built-in out branches on the name of the way out it leaves by. So the store's word is
// what arrives here, and this turns it into the screen's.
//
// **Only a row that carries a built-in's key is turned.** A person's own action may use the same word
// for a way out of its own, and that is theirs, not Amenbo's — so the key is asked for, and a word
// with no key, or one the built-in does not have, comes back as it was.
//
// **The Japanese dictionary is the table.** Its `auto.bi.*` entries are the store's words, each under
// the key every other language translates, so the word is looked up there and the key it sits under
// is read in the screen's language. The core holds the dictionary to its own words
// (`amenbo_core::ops::automation_builtin`'s test), so a word renamed on one side and not the other
// stops the build rather than showing up untranslated.
import type { AutomationBuiltinDto } from "../bindings/bindings";
import { t } from "./i18n";
import { ja } from "./i18n/locales/ja";

/** Each built-in's key, and the part of the dictionary's keys its words sit under. */
const SECTIONS: Record<string, string> = {
  take_task: "takeTask",
  cut_worktree: "cutWorktree",
  fold_worktree: "foldWorktree",
  close_task: "closeTask",
};

/** For each built-in, the store's word → the dictionary key it sits under. */
const WORDS: ReadonlyMap<string, ReadonlyMap<string, string>> = new Map(
  Object.entries(SECTIONS).map(([key, section]) => [
    key,
    new Map(
      Object.entries(ja.ui)
        .filter(([dictKey]) => dictKey.startsWith(`auto.bi.${section}.`))
        .map(([dictKey, word]) => [word as string, dictKey]),
    ),
  ]),
);

/**
 * The screen's word for `word`, where `builtin` is the key of the built-in it was written by. Absent
 * `builtin`, or a word that built-in does not have, and `word` comes back untouched.
 */
export function builtinWord(builtin: string | null | undefined, word: string): string {
  if (builtin === null || builtin === undefined) return word;
  const dictKey = WORDS.get(builtin)?.get(word);
  return dictKey === undefined ? word : t(dictKey);
}

/**
 * A built-in's definition with every word on it in the screen's language — what the library, the
 * actions tab and the screen reading one draw and search. The key stays as it came: it is what
 * placing one names.
 */
export function builtinShown(builtin: AutomationBuiltinDto): AutomationBuiltinDto {
  const word = (one: string) => builtinWord(builtin.key, one);
  return {
    ...builtin,
    name: word(builtin.name),
    does: word(builtin.does),
    settings: builtin.settings.map((one) => ({ ...one, name: word(one.name) })),
    inputs: builtin.inputs.map((one) => ({ ...one, name: word(one.name) })),
    exits: builtin.exits.map((one) => ({
      ...one,
      name: one.name === undefined ? undefined : word(one.name),
      outputs: one.outputs.map((out) => ({ ...out, name: word(out.name) })),
    })),
  };
}
