// A built-in's words come out of the store in Japanese, and are drawn in the screen's language — but
// only where the row carries the built-in's key, since a person's own action may name a way out the
// same and that word is theirs.
import { afterEach, describe, expect, it, vi } from "vitest";

const lang = vi.hoisted(() => ({ now: "en" }));
vi.mock("./snapshot", () => ({ getSnapshot: () => ({ language: lang.now, dateLocale: null }) }));

import { builtinShown, builtinWord } from "./builtinWords";
import type { AutomationBuiltinDto } from "../bindings/bindings";

afterEach(() => {
  lang.now = "en";
});

describe("a built-in's words", () => {
  it("are drawn in the screen's language where the key is given", () => {
    expect(builtinWord("take_task", "タスクに着手する")).toBe("Take a task");
    expect(builtinWord("take_task", "着手できるタスクが無い")).toBe("No task to take");
    expect(builtinWord("fold_worktree", "未マージ")).toBe("Unmerged");
    lang.now = "de";
    expect(builtinWord("take_task", "着手した")).toBe("Aufgabe übernommen");
  });

  it("stay the store's in Japanese", () => {
    lang.now = "ja";
    expect(builtinWord("take_task", "着手できるタスクが出るまで待つ")).toBe("着手できるタスクが出るまで待つ");
  });

  it("are left alone without the key, or where that built-in has no such word", () => {
    expect(builtinWord(undefined, "着手した")).toBe("着手した");
    expect(builtinWord(null, "着手した")).toBe("着手した");
    // Another built-in's word is not this one's.
    expect(builtinWord("close_task", "着手した")).toBe("着手した");
    expect(builtinWord("no_such_builtin", "着手した")).toBe("着手した");
  });

  it("are turned all at once on a definition, key and all else kept", () => {
    const take: AutomationBuiltinDto = {
      key: "take_task",
      name: "タスクに着手する",
      does: "絞り込みに合う未着手で ready のタスクを並び順どおりに探し、先頭から予約して進行中にする",
      settings: [{ name: "絞り込み", kind: "taskfilter", required: false }],
      inputs: [],
      exits: [
        { name: "着手した", outputs: [{ name: "タスク", kind: "task_take", required: true }] },
        { name: "着手できるタスクが無い", outputs: [] },
      ],
      usedBy: 2,
    };
    const shown = builtinShown(take);
    expect(shown.key).toBe("take_task");
    expect(shown.usedBy).toBe(2);
    expect(shown.name).toBe("Take a task");
    expect(shown.settings.map((one) => one.name)).toEqual(["Filter"]);
    expect(shown.exits.map((one) => one.name)).toEqual(["Took a task", "No task to take"]);
    expect(shown.exits[0].outputs.map((one) => one.name)).toEqual(["Task"]);
  });
});
