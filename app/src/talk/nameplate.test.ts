// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { t, tf } from "../core/i18n";
import { clockOf, faceOf, mountNameplate, type Dot } from "./nameplate";

/** The lamp in front of the name, at rest. These cases are about the name on the row; what the lamp
 *  does has its own (`./plateMoving.test`). */
const STILL: Dot = { hue: 199, face: "out" };

describe("the row on the page", () => {
  it("is one line, and redraws in place", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "the migration", dot: STILL, run: null });
    expect(host.querySelector(".plate__name")?.textContent).toBe("the migration");

    const row = host.querySelector(".plate");
    draw({ name: "the backup", dot: STILL, run: null });
    expect(host.querySelectorAll(".plate")).toHaveLength(1);
    // Redrawn in place: the row is not rebuilt, so nothing under the pointer moves.
    expect(host.querySelector(".plate")).toBe(row);
    expect(host.querySelector(".plate__name")?.textContent).toBe("the backup");
    // The row carries no tooltip of its own: the name in full is in the panel instead, and the
    // machine's own tooltip over that panel would be the same word twice.
    expect(host.querySelector(".plate__name")?.getAttribute("title")).toBeNull();
    expect(host.querySelector(".plate-peek__name")?.textContent).toBe("the backup");
  });

  it("says the name in full where the row had to cut it, and nothing else", () => {
    // The panel is the way back to a name the row elided. It is the name and no second reading of
    // it: a pane's row says what the pane is called, and that is the whole of what it says
    // (`AMB-D-862`).
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    const name = "the migration that moves the store and then puts the pointer back";

    draw({ name, dot: STILL, run: null });
    expect(host.querySelector(".plate-peek__name")?.textContent).toBe(name);
    expect(host.querySelector(".plate-peek")?.textContent).toBe(name);
  });

  it("keeps the panel down for a pane nothing has named", () => {
    // An empty box with a border on it reads as something having gone wrong, and a pane with no name
    // has nothing to put in one.
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: null, dot: STILL, run: null });
    expect((host.querySelector(".plate-peek") as HTMLElement).hidden).toBe(true);
  });

  it("comes down when there is nothing to label, and comes back with the same row", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw(null);
    const row = host.querySelector(".plate");
    expect(row, "the row was removed rather than hidden").toBeTruthy();
    expect((row as HTMLElement).hidden, "a pane with no session was labelled anyway").toBe(true);
    // The panel goes with the row. A pane that has never had a session has nothing to put in it, and
    // a panel left up would be an empty box with a border on it — dropped by a pointer on the row's
    // place, or by the keyboard reaching the button beside it.
    expect(
      (host.querySelector(".plate-peek") as HTMLElement).hidden,
      "a pane with no session kept an empty panel",
    ).toBe(true);

    draw({ name: null, dot: STILL, run: null });
    expect(host.querySelector(".plate")).toBe(row);
    expect((row as HTMLElement).hidden).toBe(false);
  });
});

describe("the row above a run's pane", () => {
  /** Where a run has got to, as the face hands it over (`./nameplate`). */
  const RUN = {
    automation: "家計簿の開発ループ",
    run: 7,
    step: "取る",
    automationId: 3,
    placement: 11,
    box: null,
    builtin: false,
    interactive: false,
    by: null,
    command: null,
    startedAt: null,
    action: "下ごしらえ",
    task: { ref: "AMB-T-5252", title: "ペインのヘッダを描く", seq: 2 },
    state: null,
  };
  const STATE = { why: null, exit: null, errorExit: false, acknowledged: false, pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false };

  it("marks a built-in's step with the chip every screen marks one with, and no other step", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: RUN });
    expect(host.querySelector<HTMLElement>(".plate__builtin")?.hidden).toBe(true);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, builtin: true } });
    const chip = host.querySelector<HTMLElement>(".plate__builtin")!;
    expect(chip.hidden).toBe(false);
    expect(chip.textContent).toBe(t("auto.actions.reachBuiltin"));
  });

  it("says which run on the first line, which step on the second, and which task on the third", () => {
    // All of them are Amenbo's own — off the execution row and off the ledger — which is the whole
    // of why they can be said at all (`AMB-D-858`). What the step printed is not among them: that is
    // in the terminal under the row.
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: RUN });

    expect(host.querySelector(".plate__auto")?.textContent).toBeTruthy();
    expect((host.querySelector(".plate__auto") as HTMLElement).hidden).toBe(false);
    // The first line holds the run and the state, nothing more (`AMB-T-5739`): the step is the
    // second line's, and the count of tasks is the task line's. The lamp is not drawn: beside the
    // glowing mark it read as a second mark saying running.
    const row = host.querySelector(".plate") as HTMLElement;
    expect([...row.children].map((el) => el.className))
      .toEqual(["plate__dot", "plate__auto", "plate__name", "plate__no", "plate__state"]);
    expect((host.querySelector(".plate__dot") as HTMLElement).hidden).toBe(true);
    expect(row.classList.contains("plate--run")).toBe(true);
    // The step's line: the step, the chip drawn on a built-in's step alone, who carries the step
    // out, the command a script runs, and how long the step has run.
    const stepLine = host.querySelector(".plate-step") as HTMLElement;
    expect(stepLine.hidden).toBe(false);
    expect([...stepLine.children].map((el) => el.classList[0]))
      .toEqual(["plate__step", "plate__builtin", "plate__by", "plate__command", "plate__elapsed"]);
    expect(host.querySelector(".plate__no")?.textContent).toBe("#7");
    // Which step, with the action its spot stands on where that is not the step's own name
    // (`AMB-D-949`). The step's own name is the part drawn heavier.
    expect(host.querySelector(".plate__step")?.textContent).toBe("下ごしらえ›取る");
    expect(host.querySelector(".plate__step b")?.textContent).toBe("取る");
    // The task line: the reference, the title, and how many tasks in — with no label, the reference
    // being what says it is a task.
    const line = host.querySelector(".plate-run") as HTMLElement;
    expect([...line.children].map((el) => el.className))
      .toEqual(["plate-run__task", "plate-run__title", "plate-run__nth"]);
    expect(host.querySelector(".plate-run__task")?.textContent).toBe("AMB-T-5252");
    expect(host.querySelector(".plate-run__title")?.textContent).toBe("ペインのヘッダを描く");
    expect(host.querySelector(".plate-run__nth")?.textContent).toBe(tf("face.runTask", { n: 2 }));
    expect(host.querySelector(".plate-peek__task")?.textContent)
      .toBe("AMB-T-5252 ペインのヘッダを描く");
  });

  /// Who carries the step out is the run's own value for it (`AMB-D-858`): the agent and its model
  /// as the run copied them, said the way the test pane says them.
  it("says which agent carries the step out, with its model where it was given one", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, by: { kind: "agent", agent: "claude-code", model: "opus" } } });
    const chip = host.querySelector<HTMLElement>(".plate__by")!;
    expect(chip.hidden).toBe(false);
    expect(chip.textContent).toBe("claude-code · opus");
    expect(host.querySelector(".plate__command")?.textContent).toBe("");

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, by: { kind: "agent", agent: "codex-cli", model: null } } });
    expect(chip.textContent).toBe("codex-cli");

    // A built-in has a chip of its own, and says nothing here.
    draw({ name: "/work/a", dot: STILL, run: { ...RUN, builtin: true } });
    expect(chip.hidden).toBe(true);
  });

  /// A script's command is its program and its arguments as they were written (`AMB-D-1016`). The
  /// row cuts it where the pane is narrow, and the panel says it whole.
  it("says a script's whole command on the step's line and in the panel", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    const command = "/usr/bin/python3 scripts/check.py --strict";

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, by: { kind: "script" }, command } });
    expect(host.querySelector(".plate__by")?.textContent).toBe(t("auto.step.byScript"));
    expect(host.querySelector(".plate__command")?.textContent).toBe(command);
    expect(host.querySelector(".plate-peek__command")?.textContent).toBe(command);

    // An agent's step has no command, and the panel says none.
    draw({ name: "/work/a", dot: STILL, run: RUN });
    expect(host.querySelector(".plate-peek__command")?.textContent).toBe("");
  });

  /// The time is counted from when the step was opened, up to the moment the row is drawn for.
  it("says how long the step has run, and nothing before it is known when it started", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    const at = Date.parse("2026-10-05T04:00:00Z");

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, startedAt: at }, now: at + 65_000 });
    expect(host.querySelector(".plate__elapsed")?.textContent).toBe(tf("face.elapsed", { time: "1:05" }));

    draw({ name: "/work/a", dot: STILL, run: RUN });
    expect(host.querySelector(".plate__elapsed")?.textContent).toBe("");
  });

  it("writes a span of time the way a clock does", () => {
    expect(clockOf(0)).toBe("0:00");
    expect(clockOf(59_999)).toBe("0:59");
    expect(clockOf(605_000)).toBe("10:05");
    expect(clockOf(3_725_000)).toBe("1:02:05");
    // A clock a little behind the host's never counts below nothing.
    expect(clockOf(-2_000)).toBe("0:00");
  });

  /// The step is numbered as the picture numbers its box (`AMB-T-5538`) — and where that number is not
  /// known, the row says the step without one rather than a count the picture does not draw.
  it("numbers the step with its box on the picture, and with nothing while that is not known", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, box: 2 } });
    expect(host.querySelector(".plate__step .plate__box")?.textContent).toBe("2");
    expect(host.querySelector(".plate__step")?.textContent).toBe("2下ごしらえ›取る");

    draw({ name: "/work/a", dot: STILL, run: RUN });
    expect(host.querySelector(".plate__step .plate__box")).toBeNull();
  });

  /// An action of one step is most often named after it, and the same word twice says nothing.
  it("says the step alone where the action is named after it", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, action: "取る" } });

    expect(host.querySelector(".plate__step")?.textContent).toBe("取る");
  });

  /// A spot taken off the picture while its run walks on leaves the step with nothing to be inside
  /// of. The row says the step alone rather than a name the picture no longer holds.
  it("says the step alone where the spot it was opened from has gone", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, action: null } });

    expect(host.querySelector(".plate__step")?.textContent).toBe("取る");
    expect(host.querySelector(".plate__step b")?.textContent).toBe("取る");
  });

  /// A step that takes its task opens on none. The task's line is left up and empty rather than
  /// taken down, so the header does not grow a line under the terminal when the task comes
  /// (`AMB-T-5739`).
  it("keeps the task line, empty, while the run is on no task", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, task: null } });

    expect((host.querySelector(".plate-run") as HTMLElement).hidden).toBe(false);
    expect(host.querySelector(".plate-run")?.textContent).toBe("");
    expect((host.querySelector(".plate-step") as HTMLElement).hidden).toBe(false);
  });

  /// Until the run has been read the row says nothing of its state: a mark guessed from the step
  /// would say "running" of a run that had already ended.
  it("says no state before the run has been read", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: RUN });

    expect((host.querySelector(".plate__state") as HTMLElement).hidden).toBe(true);
  });

  /// A run's pane outlives its last step, so what the row says has to move with the run and not
  /// with the step — a completed run's pane was left naming the step it ended on (`AMB-T-5506`).
  it("says where the run stands, and marks which state for the drawing", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, state: { ...STATE, status: "paused", word: "一時停止中" } } });
    const state = host.querySelector(".plate__state") as HTMLElement;
    expect(state.hidden).toBe(false);
    expect(state.textContent).toBe("一時停止中");
    expect(state.dataset.state).toBe("paused");

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, state: { ...STATE, status: "completed", word: "完了" } } });
    expect(state.textContent).toBe("完了");
    expect(state.dataset.state).toBe("completed");
  });

  /// Running is said by the mark glowing rather than by a chip (`AMB-D-1010`).
  it("says running by the mark glowing, and draws the mark still once the run is held or over", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    const state = () => host.querySelector(".plate__state") as HTMLElement;
    const auto = () => host.querySelector(".plate__auto") as HTMLElement;

    draw({ name: "/work/a", dot: STILL, run: RUN });
    expect(auto().dataset.run).toBeUndefined();

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, state: { ...STATE, status: "running", word: "実行中" } } });
    expect(state().hidden).toBe(true);
    expect(auto().dataset.run).toBe("on");

    // A pause on its way, and a run waiting for its next task, are still running: the mark glows, and
    // the chip says which.
    draw({ name: "/work/a", dot: STILL, run: { ...RUN, state: { ...STATE, status: "running", word: "一時停止待ち", pauseRequested: true } } });
    expect(state().hidden).toBe(false);
    expect(state().textContent).toBe("一時停止待ち");
    expect(auto().dataset.run).toBe("on");

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, state: { ...STATE, status: "running", word: "一時停止待ち", pauseBeforeNextTask: true } } });
    expect(state().hidden).toBe(false);
    expect(state().textContent).toBe("一時停止待ち");

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, state: { ...STATE, status: "running", word: "タスク待ち", waiting: true } } });
    expect(state().hidden).toBe(false);
    expect(state().textContent).toBe("タスク待ち");
    expect(auto().dataset.run).toBe("on");

    for (const status of ["paused", "completed", "failed", "canceled"] as const) {
      draw({ name: "/work/a", dot: STILL, run: { ...RUN, state: { ...STATE, status, word: status } } });
      expect(state().hidden).toBe(false);
      expect(auto().dataset.run).toBe("still");
    }
  });

  it("says nothing of a run on a pane that is not one", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "the migration", dot: STILL, run: null });

    expect((host.querySelector(".plate-run") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate-step") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate__auto") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate__dot") as HTMLElement).hidden).toBe(false);
    expect((host.querySelector(".plate__no") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate__state") as HTMLElement).hidden).toBe(true);
    // The panel is the name's, exactly as it was: an empty line in it would push it taller for a
    // pane that has no run.
    expect(host.querySelector(".plate-peek")?.textContent).toBe("the migration");
  });

  it("drops the run's line when the pane goes back to having no row", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: RUN });
    draw(null);

    expect((host.querySelector(".plate-run") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate-step") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate-peek") as HTMLElement).hidden).toBe(true);
  });
});

describe("which face the lamp is on", () => {
  it("is lit while output is arriving, and out when it is not", () => {
    // The stream and nothing else. What silence means is not knowable from outside, so the lamp does
    // not try to say (`AMB-D-858`, `AMB-D-862`).
    expect(faceOf(true)).toBe("lit");
    expect(faceOf(false)).toBe("out");
  });
});
