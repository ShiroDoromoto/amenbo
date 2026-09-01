// A Tauri command reports failure as a structured CmdError (src-tauri/error.rs). The front end maps the code to a
// per-language template, falling back to the sentence core wrote — English, whoever is reading — for codes that
// have no template (`AMB-D-413`).
import { describe, it, expect } from "vitest";
import { CORE_CLI_ONLY_ERROR_CODES, CORE_SENTENCE_ERROR_CODES, TAURI_ERROR_CODES } from "../errorCodes";
import { DICTIONARIES, errLabel, errText, type CmdError } from "./index";
import { en } from "./locales/en";

const bindingStale: CmdError = {
  code: "binding_stale",
  message_en: "the linked project directory was not found: /gone",
  fields: { path: "/gone" },
};

const ambiguous: CmdError = {
  code: "ambiguous_id",
  message_en: "id 'ab' is ambiguous. candidates: [\"abc\", \"abd\"]",
  fields: { prefix: "ab", candidates: ["abc", "abd"] },
};

// A free-form variant (no template). It simply falls back to the sentence core wrote.
const notFound: CmdError = {
  code: "not_found",
  message_en: "task 'X' not found",
  fields: null,
};

// The same failure once core names the sentence rather than the family: the id rides in the fields, and
// the prose the reader gets is written here rather than in Rust (`AMB-D-413`).
const notFoundTask: CmdError = {
  code: "not_found_task",
  message_en: "task 'AMB-T-12' not found",
  fields: { ref: "AMB-T-12" },
};

// A refusal the Tauri layer raises itself. It carries no Japanese — the sentence a Japanese reader
// gets is the template here, built from the fields.
const nestedTree: CmdError = {
  code: "binding_nested_tree",
  message_en: "this folder is already inside an Amenbo-managed tree (bound at /work/repo); binding a subfolder would shadow that pointer",
  fields: { path: "/work/repo" },
};

// A refusal that is one sentence over a list of reasons. Core sends the reasons as parts, each naming
// its own sentence, because how many there are is known only at the moment of refusing.
const notReady: CmdError = {
  code: "not_ready",
  message_en:
    "cannot reserve task AMB-T-12: blocker AMB-T-9 is not done; premise AMB-D-4 is not settled — wait for the ruling, or unlink it",
  fields: { ref: "AMB-T-12" },
  parts: [
    { code: "not_ready_open_blocker", message_en: "blocker AMB-T-9 is not done", fields: { ref: "AMB-T-9" } },
    {
      code: "not_ready_premise_unsettled",
      message_en: "premise AMB-D-4 is not settled — wait for the ruling, or unlink it",
      fields: { ref: "AMB-D-4" },
    },
  ],
};

// The store engine failing. Its own words name a database library nobody asked about, so the reader gets
// the template instead — the raw sentence belongs in the diagnostic log.
const storageError: CmdError = {
  code: "storage_error",
  message_en: "store (engine) operation failed: disk I/O error",
  fields: null,
};

describe("errLabel", () => {
  it("interpolates a code that has a template with its fields (per language)", () => {
    expect(errLabel(bindingStale, "ja")).toBe("プロジェクトの紐付け先ディレクトリが見つかりません: /gone");
    expect(errLabel(bindingStale, "en")).toBe("The linked project directory was not found: /gone");
  });

  it("interpolates array fields as a comma-separated list", () => {
    expect(errLabel(ambiguous, "ja")).toContain("候補: abc, abd");
    expect(errLabel(ambiguous, "en")).toContain("(abc, abd)");
  });

  it("writes a Tauri-raised refusal in the reader's language, from its fields alone", () => {
    expect(errLabel(nestedTree, "ja")).toBe(
      "このフォルダは既に Amenbo の管理ツリーの中にあります（/work/repo で紐付け済み）。サブフォルダを紐付けると上位の目印（.amenbo）が隠れます。",
    );
    expect(errLabel(nestedTree, "en")).toContain("bound at /work/repo");
    // The English it arrives with is what a reader gets only where no template exists, so the
    // Japanese above must not be it.
    expect(errLabel(nestedTree, "ja")).not.toBe(nestedTree.message_en);
  });

  it("a code with no template falls back to the sentence core wrote, whatever the reader's language", () => {
    expect(errLabel(notFound, "ja")).toBe("task 'X' not found");
    expect(errLabel(notFound, "en")).toBe("task 'X' not found");
  });

  it("writes each of a refusal's reasons from its own template and joins them the language's way", () => {
    expect(errLabel(notReady, "en")).toBe(
      "AMB-T-12 cannot be reserved yet: AMB-T-9 is not done; AMB-D-4 is not settled — wait for the ruling, or unlink it",
    );
    // Japanese joins a list with its own mark, and nothing of the English survives.
    expect(errLabel(notReady, "ja")).toBe(
      "AMB-T-12 はまだ予約できません: AMB-T-9 が完了していません、AMB-D-4 が未確定です。裁定を待つかリンクを外してください",
    );
    expect(errLabel(notReady, "ja")).not.toContain("is not done");
  });

  it("a reason whose code has no template falls back to its own English, not the whole message", () => {
    const oneOff: CmdError = {
      ...notReady,
      parts: [{ code: "not_a_code", message_en: "something else stands in the way", fields: null }],
    };
    expect(errLabel(oneOff, "ja")).toBe("AMB-T-12 はまだ予約できません: something else stands in the way");
  });

  it("keeps the store engine's own words off the screen, in every language", () => {
    for (const lang of Object.keys(DICTIONARIES) as (keyof typeof DICTIONARIES)[]) {
      const line = errLabel(storageError, lang);
      expect(line, lang).not.toContain("disk I/O error");
      expect(line, lang).toContain("Amenbo");
    }
  });

  it("writes a named sentence from its fields, in a language core carries no prose for", () => {
    expect(errLabel(notFoundTask, "en")).toBe("Task AMB-T-12 was not found.");
    expect(errLabel(notFoundTask, "ja")).toBe("タスク AMB-T-12 が見つかりません。");
    // The third language is the whole point: core holds no Korean, so a sentence only exists there if
    // the template does.
    expect(errLabel(notFoundTask, "ko")).toContain("AMB-T-12");
    expect(errLabel(notFoundTask, "ko")).not.toBe(notFoundTask.message_en);
  });
});

describe("errText", () => {
  it("localizes a structured CmdError via errLabel", () => {
    expect(errText({ ...bindingStale, fields: { path: "/nope" } })).toContain("/nope");
  });

  it("renders bare strings, Errors, and anything else as a single line as-is", () => {
    expect(errText("plain string error")).toBe("plain string error");
    expect(errText(new Error("boom"))).toBe("boom");
    expect(errText(42)).toBe("42");
  });

  it("a non-CmdError object falls back to String() (the check that avoids [object Object])", () => {
    // An object that does not carry both code and message_en is not treated as a structured error.
    expect(errText({ foo: "bar" })).toBe("[object Object]");
  });
});

// A code core raises carries an English sentence of its own, so a missing template still reads.
// A code raised here does not: nothing else holds a sentence for it, and without a template the
// reader would be shown the bare English one whatever language they are in. So every one of them
// must have a template, and adding a refusal means adding its sentence in the same breath.
describe("the codes this layer raises itself", () => {
  it("all have an English template, which is where their sentence lives", () => {
    const missing = TAURI_ERROR_CODES.filter((code) => !en.err[code]);
    expect(missing).toEqual([]);
  });
});

// A sentence code exists for one reason: a template can be written for it. Splitting one off its family
// and then leaving the dictionary empty gains nothing — the reader lands back on core's English, which is
// exactly where the coarse code already left them. So the split and the sentence arrive together, and a
// new one added without its prose fails here rather than reading as translated.
describe("the codes core splits down to one sentence", () => {
  it("all have an English template", () => {
    const missing = CORE_SENTENCE_ERROR_CODES.filter((code) => !en.err[code]);
    expect(missing).toEqual([]);
  });
});

// The other side of the same rule. A code no screen can reach owes no sentence, and writing one anyway
// costs every language a string nobody will ever be shown — a cost paid again at each pass over the
// dictionaries. A code that starts reaching a screen moves to the sentence list, and its prose arrives
// with the move; this fails first if the template arrives without it.
describe("the codes only the CLI refuses with", () => {
  it("have no template, in English or anywhere else", () => {
    const written = Object.entries(DICTIONARIES).flatMap(([lang, dict]) =>
      CORE_CLI_ONLY_ERROR_CODES.filter((code) => dict.err[code]).map((code) => `${lang}: ${code}`),
    );
    expect(written).toEqual([]);
  });
});
