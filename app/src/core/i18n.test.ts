// A Tauri command reports failure as a structured CmdError (src-tauri/error.rs). The front end maps the code to a
// per-language template, falling back to message (ja) / message_en (en) for codes that have no template.
import { describe, it, expect } from "vitest";
import { errLabel, errText, type CmdError } from "./i18n";

const bindingStale: CmdError = {
  code: "binding_stale",
  message: "プロジェクトの紐付け先ディレクトリが見つかりません: /gone",
  message_en: "the linked project directory was not found: /gone",
  fields: { path: "/gone" },
};

const ambiguous: CmdError = {
  code: "ambiguous_id",
  message: "ID 'ab' は曖昧です。候補: [\"abc\", \"abd\"]",
  message_en: "id 'ab' is ambiguous. candidates: [\"abc\", \"abd\"]",
  fields: { prefix: "ab", candidates: ["abc", "abd"] },
};

// A free-form variant (no template). It simply falls back to message/message_en.
const notFound: CmdError = {
  code: "not_found",
  message: "タスク 'X' が見つかりません",
  message_en: "task 'X' not found",
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

  it("a code with no template falls back to the per-language message", () => {
    expect(errLabel(notFound, "ja")).toBe("タスク 'X' が見つかりません");
    expect(errLabel(notFound, "en")).toBe("task 'X' not found");
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
    // An object that does not carry all of code/message/message_en is not treated as a structured error.
    expect(errText({ foo: "bar" })).toBe("[object Object]");
  });
});
