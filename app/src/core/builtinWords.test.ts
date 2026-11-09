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
    expect(builtinWord("take_task", "着手した")).toBe("Took a task");
    expect(builtinWord("fold_worktree", "未マージ")).toBe("Unmerged");
    expect(builtinWord("hand_back_task", "タスクを人に返す")).toBe("Hand the task back to a person");
    expect(builtinWord("defer_task", "タスクを後回しにする")).toBe("Put the task off");
    lang.now = "de";
    expect(builtinWord("take_task", "着手した")).toBe("Aufgabe übernommen");
  });

  it("stay the store's in Japanese", () => {
    lang.now = "ja";
    expect(builtinWord("take_task", "着手した")).toBe("着手した");
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
      does: "絞り込みに合い、着手できる未着手のタスクを並び順どおりに探し、先頭から予約して進行中にする",
      steps: ["着手できるタスクが無ければ、出るまで待つ。この実行はここで終わり、タスクが出たら、この役割の空いた席で次の実行が着手する"],
      halts: ["着手できるタスクがあったのに、予約する前にすべてほかで予約された"],
      settings: [{ name: "絞り込み", kind: "taskfilter", required: false }],
      inputs: [],
      exits: [
        { name: "着手した", outputs: [{ name: "タスク", kind: "task_take", required: true }] },
      ],
      usedBy: 2,
    };
    const shown = builtinShown(take);
    expect(shown.key).toBe("take_task");
    expect(shown.usedBy).toBe(2);
    expect(shown.name).toBe("Take a task");
    expect(shown.steps).toEqual([
      "If there is none to take, it waits until one turns up. This run ends here, and once a task turns up the next run takes it in the role’s free seat",
    ]);
    expect(shown.halts).toEqual([
      "There was a task to take, but every one was reserved elsewhere before it could reserve it",
    ]);
    expect(shown.settings.map((one) => one.name)).toEqual(["Filter"]);
    expect(shown.exits.map((one) => one.name)).toEqual(["Took a task"]);
    expect(shown.exits[0].outputs.map((one) => one.name)).toEqual(["Task"]);
  });
});
