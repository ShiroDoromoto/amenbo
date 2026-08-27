// What a language's editing manners are, and where they come from.
//
// One file per language under `./langconfig/`, baked out of VS Code's built-in extensions by
// `scripts/gen-lang-config.mjs` and tracked (`AMB-D-769`). Nobody running Amenbo has VS Code; what
// is read here is a plain JSON file that shipped with the app.
//
// **A language with no file is not a language with no manners.** Everything falls back to the
// generic rule below, which is worth 85% of the indentation in a brace language on its own
// (`AMB-T-3745`) — the configuration buys the last stretch, not the whole of it.
//
// Nothing here accepts a configuration from outside the app, and that is a security line rather
// than an omission: CodeMirror asks for indentation and folds synchronously, inside a transaction,
// so a runaway regular expression in somebody else's rules has nowhere to be sent and would simply
// stop the window (`AMB-D-769`).

/** A pair the editor can close or wrap a selection in. */
export type Pair = {
  open: string;
  close: string;
  /** Token kinds this pair must not be closed inside — VS Code's own vocabulary. */
  notIn?: string[];
};

/**
 * A regular expression as a configuration writes one.
 *
 * Two shapes are in the files at once: the older ones write the source as a bare string, the newer
 * ones wrap it so they can carry flags beside it. Both are read here rather than flattened on the
 * way in, so the tracked file stays the thing VS Code publishes.
 */
export type Pattern = string | { pattern: string; flags?: string };

/** One language's manners, as the baked file holds them. */
export type LangConfig = {
  comments?: { lineComment?: string; blockComment?: [string, string] };
  brackets?: [string, string][];
  autoClosingPairs?: (Pair | [string, string])[];
  surroundingPairs?: (Pair | [string, string])[];
  indentationRules?: {
    increaseIndentPattern?: Pattern;
    decreaseIndentPattern?: Pattern;
    indentNextLinePattern?: Pattern;
    unIndentedLinePattern?: Pattern;
  };
  folding?: { offSide?: boolean; markers?: { start: Pattern; end: Pattern } };
  onEnterRules?: {
    beforeText: Pattern;
    afterText?: Pattern;
    previousLineText?: Pattern;
    action: { indent: "none" | "indent" | "outdent" | "indentOutdent"; appendText?: string; removeText?: number };
  }[];
};

// What a line ending in an opening bracket, and a line starting with a closing one, mean — with no
// language named. It is a small rule and it does most of the work: on its own it puts 85.6% of
// Rust's lines and 67.6% of TypeScript's where the formatter put them, and what a language's own
// configuration buys is the last stretch rather than the whole of it (`AMB-T-3745`).
const BY_BRACKET = {
  increaseIndentPattern: "[{\\[(]\\s*$",
  decreaseIndentPattern: "^\\s*[}\\])]",
};

// The manners of a language nothing was baked for.
const GENERIC: LangConfig = {
  brackets: [["{", "}"], ["[", "]"], ["(", ")"]],
  autoClosingPairs: [["{", "}"], ["[", "]"], ["(", ")"]],
  surroundingPairs: [["{", "}"], ["[", "]"], ["(", ")"], ['"', '"'], ["'", "'"]],
  indentationRules: BY_BRACKET,
};

// One import each, so a language's manners are fetched with its grammar and never before. `toml`
// has no entry: VS Code ships no TOML extension, so it takes the generic rule like the languages
// nothing was ever written for.
const FILES: Partial<Record<string, () => Promise<{ default: unknown }>>> = {
  css: () => import("./langconfig/css.json"),
  go: () => import("./langconfig/go.json"),
  html: () => import("./langconfig/html.json"),
  json: () => import("./langconfig/json.json"),
  markdown: () => import("./langconfig/markdown.json"),
  python: () => import("./langconfig/python.json"),
  rust: () => import("./langconfig/rust.json"),
  shellscript: () => import("./langconfig/shellscript.json"),
  sql: () => import("./langconfig/sql.json"),
  tsx: () => import("./langconfig/tsx.json"),
  yaml: () => import("./langconfig/yaml.json"),
};

/** The manners of `lang`, or the generic ones for a language nothing was baked for. */
export async function configFor(lang: string): Promise<LangConfig> {
  const file = FILES[lang];
  if (file === undefined) return GENERIC;
  try {
    // The shape is the generator's to guarantee: TypeScript reads a JSON file literally and calls
    // `["{", "}"]` a `string[]`, which no hand-written type can be widened to accept without giving
    // up saying a bracket comes in twos.
    return (await file()).default as LangConfig;
  } catch {
    // A configuration that fails to arrive costs manners, not the editor: the generic rule is what
    // most of what a folder holds runs on anyway.
    return GENERIC;
  }
}

/** A pair written either as an object or as the two-element array the older files use. */
export function asPair(one: Pair | [string, string]): Pair {
  return Array.isArray(one) ? { open: one[0], close: one[1] } : one;
}

/** The regular expressions of one configuration, built once instead of per keystroke. */
export type Rules = {
  increase: RegExp | null;
  decrease: RegExp | null;
  indentNext: RegExp | null;
  unIndented: RegExp | null;
  foldStart: RegExp | null;
  foldEnd: RegExp | null;
  closing: Pair[];
  surrounding: Pair[];
  brackets: [string, string][];
  /** What pressing Enter does, where the language has something to say about it. */
  onEnter: OnEnter[];
};

/** One rule about pressing Enter, with its patterns already compiled. */
export type OnEnter = {
  before: RegExp;
  after: RegExp | null;
  previous: RegExp | null;
  indent: "none" | "indent" | "outdent" | "indentOutdent";
  appendText: string;
  removeText: number;
};

// A pattern that will not compile is dropped rather than thrown: the rest of the configuration is
// still worth having, and the alternative is an editor that refuses to open the file.
export function re(pattern: Pattern | undefined): RegExp | null {
  if (pattern === undefined) return null;
  const wrapped = typeof pattern === "string" ? { pattern } : pattern;
  try {
    return new RegExp(wrapped.pattern, wrapped.flags);
  } catch {
    return null;
  }
}

/**
 * Compile one configuration into the form the manners are actually run from.
 *
 * A language whose file says nothing about indentation — Markdown, SQL, the shell — falls to the
 * bracket rule rather than to nothing, the same as a language with no file at all.
 */
export function compile(config: LangConfig): Rules {
  const indentation = config.indentationRules ?? BY_BRACKET;
  return {
    increase: re(indentation.increaseIndentPattern),
    decrease: re(indentation.decreaseIndentPattern),
    indentNext: re(config.indentationRules?.indentNextLinePattern),
    unIndented: re(config.indentationRules?.unIndentedLinePattern),
    foldStart: re(config.folding?.markers?.start),
    foldEnd: re(config.folding?.markers?.end),
    // A rule whose own pattern will not compile is dropped, the same as any other: what it would
    // have done on Enter is exactly what happens without it.
    onEnter: (config.onEnterRules ?? []).flatMap((rule) => {
      const before = re(rule.beforeText);
      if (before === null) return [];
      return [{
        before,
        after: re(rule.afterText),
        previous: re(rule.previousLineText),
        indent: rule.action.indent,
        appendText: rule.action.appendText ?? "",
        removeText: rule.action.removeText ?? 0,
      }];
    }),
    closing: (config.autoClosingPairs ?? GENERIC.autoClosingPairs ?? []).map(asPair),
    surrounding: (config.surroundingPairs ?? GENERIC.surroundingPairs ?? []).map(asPair),
    brackets: config.brackets ?? GENERIC.brackets ?? [],
  };
}
