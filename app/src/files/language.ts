// One language, assembled: colour, indentation, folding and brackets, over one shared reading of
// the file's tokens.
//
// **Two languages take their indentation from somewhere else.** Python and YAML close a block by
// coming back out rather than by closing a bracket, and a rule can only say "one deeper" or "one
// shallower on this line" — "one shallower from here on" is not sayable in that vocabulary, which
// is why VS Code's own Python configuration carries no indentation rules at all. Held to what the
// formatters wrote, the rules reach the low 70s in both where a parse tree reaches the 90s, and
// rewriting the rules does not close it (`AMB-T-3745`). So those two, and only those two, bring a
// parser (`AMB-D-769`).
//
// They are put together here rather than each fetching what it needs because they all need the same
// thing — where in the file the strings and the comments are — and tokenizing a document four times
// would cost four times as much for one answer (`./tmdoc`).
//
// Nothing in here is required for a file to open — every piece is behind the same dynamic import as
// the editor itself. And a language nothing was baked for is not a language with no manners: the
// generic bracket rule alone puts 85% of the lines of a brace language where its formatter put them
// (`AMB-T-3745`).

import { foldGutter, foldKeymap, indentOnInput } from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import { brackets } from "./brackets";
import { folding } from "./fold";
import { loadGrammar, type LangId } from "./grammars";
import { painting } from "./highlight";
import { compile, configFor } from "./langconfig";
import { indenting } from "./indent";
import { tmDocField } from "./tmdoc";

// Which languages are parsed for their indentation, and what parses them. The list lives here
// rather than beside either half, because "which of the two answers for this language" is one
// question and it is asked once — the colour is TextMate's for every language, this included.
//
// What arrives is a `Language`, not a `LanguageSupport`: the support bundles a completion source
// this window has no use for, and the language on its own paints nothing. The tree it builds is
// read by CodeMirror's own indentation, which falls to it when no rule service answers.
const PARSED: Partial<Record<LangId, () => Promise<Extension>>> = {
  python: async () => (await import("@codemirror/lang-python")).pythonLanguage,
  yaml: async () => (await import("@codemirror/lang-yaml")).yamlLanguage,
};

/**
 * Everything the editor should do differently because the file is written in `lang`.
 *
 * `null` means a file nothing here reads, and it is not nothing: folding is a count of the spaces at
 * the head of each line and needs no grammar at all, so a plain text file gets it too. What it does
 * not get is colour, and the brackets — those need to know where the strings are, and without a
 * grammar the answer would be a guess (`./brackets`).
 */
export async function language(lang: LangId | null): Promise<Extension> {
  if (lang === null) return [folding(compile({})), foldGutter(), keymap.of(foldKeymap)];

  const parser = PARSED[lang];
  const [{ grammar, initial }, config, parsed] = await Promise.all([
    loadGrammar(lang),
    configFor(lang),
    parser === undefined ? Promise.resolve(null) : parser(),
  ]);
  const rules = compile(config);
  const field = tmDocField(grammar, initial);

  return [
    field,
    painting(field),
    // One or the other, never both: an indentation service answers before a parse tree is ever
    // consulted, so leaving the rules in would be leaving the parser out.
    parsed ?? [
      indenting(field, rules),
      ...(rules.decrease === null ? [] : [
        EditorState.languageData.of(() => [{ indentOnInput: rules.decrease }]),
      ]),
    ],
    // The rules that have to run as the character lands rather than at the next line break: `}` and
    // `else` pull their own line back out as they are written. Both halves declare theirs the same
    // way, so this is installed once for either.
    indentOnInput(),
    folding(rules),
    foldGutter(),
    keymap.of(foldKeymap),
    brackets(field, rules),
  ];
}
