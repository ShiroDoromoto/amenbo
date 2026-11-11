// @vitest-environment jsdom
// The one control a pane has, and the two things it must not do quietly.
//
// It removes the **place**, not the program in it (`../talk/layout`): a pane going away never ends a
// session on its own — that is a pane moving, and the session outlives it (`AMB-D-753`) — so this is
// the only thing on the face that takes a frame off it for good. What is pinned here is that it asks
// before it happens, that a refusal leaves everything exactly as it was, and that going through with
// it ends whatever was running: a session whose place has gone is one nobody can reach.
//
// It is drawn whether or not a terminal is running, because a frame kept from the last run has no
// session and is still a place somebody has to be able to get rid of.
//
// **On a launch's pane it stops the launch** (`AMB-D-1154`): the pane is always asked about there —
// a seat in front going, or none yet — and a yes stops the launch before the place goes. A launch
// already ended is not stopped again. A run's own pane, outside a launch, cannot be taken away while
// its run is going.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationLaunchStateDto } from "../bindings/bindings";
import type { Say } from "../talk/nameplate";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host: open a session, then end it. */
  events: null as PaneEvents | null,
  /** The sessions the control asked the host to end. */
  ended: [] as string[],
  /** Whether the person said yes when asked. */
  agrees: true,
  /** How many times they were asked. */
  asked: 0,
  /** Where set, the question stays open until this lets it go. */
  held: null as Promise<void> | null,
  /** What was asked, in the order it was asked. */
  questions: [] as string[],
  /** The launch the pane draws, as the host answers it. */
  launched: null as AutomationLaunchStateDto | null,
  /** The launches the pane asked the host to stop, and what else happened, in order. */
  log: [] as string[],
  /** Where set, the host refuses the stop with this. */
  refuses: null as string | null,
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (_host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    return Promise.resolve(() => {});
  },
}));
// Stubbed except for what the pane reads while it draws: the box under it asks the terminal how
// tall it may be on every render (`../talk/terminal`), and a mock that dropped that answer would
// fail the pane on the mock rather than on anything this is about.
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async (session: string) => {
    hoisted.ended.push(session);
    hoisted.log.push(`end ${session}`);
  }),
  pasteIntoTerminal: vi.fn(async () => {}),
}));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async (question: string) => {
    hoisted.asked++;
    hoisted.questions.push(question);
    if (hoisted.held !== null) await hoisted.held;
    return hoisted.agrees;
  }),
}));
vi.mock("../core/automations", async (actual) => ({
  ...(await actual<typeof import("../core/automations")>()),
  useLaunchState: (launch: number | null) => (launch === null ? null : hoisted.launched),
  stopLaunch: vi.fn(async (launch: number) => {
    if (hoisted.refuses !== null) throw new Error(hoisted.refuses);
    hoisted.log.push(`stop ${launch}`);
  }),
}));
// What a launch's pane draws beside the control reads these; what they say is not what this is about.
vi.mock("../talk/runOutputs", () => ({ useRunOutputs: () => new Map() }));
vi.mock("../core/projectRoles", () => ({ useProjectRoles: () => [] }));
// The label is a live thing of its own; what it draws is not what this is about.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {}, took: () => {}, stated: () => {}, numbered: () => {}, timed: () => {},
    focused: () => {}, stop: () => {}, lastOutput: () => null,
  }),
}));

let container: HTMLDivElement;
let root: Root;
/** The frames the control asked the face to take away. */
let dropped: string[];

beforeEach(() => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test about what is drawn beside the panes says it is on a wide screen.
  window.innerWidth = 1600;
  hoisted.events = null;
  hoisted.ended = [];
  hoisted.agrees = true;
  hoisted.asked = 0;
  hoisted.held = null;
  hoisted.questions = [];
  hoisted.launched = null;
  hoisted.log = [];
  hoisted.refuses = null;
  dropped = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane, either one that starts a terminal as soon as it is drawn or a place kept from the last run
 *  with nothing in it. */
async function pane(
  autoStart = true,
  { run = null, launch = null }: { run?: Say | null; launch?: number | null } = {},
): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: "1",
      run,
      launch,
      hue: 199,
      project: 1,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart,
      focused: true,
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: (frame: string) => {
        dropped.push(frame);
        hoisted.log.push(`drop ${frame}`);
      },
      onName: () => {},
      written: "",
      onWrite: () => {},
      // Nothing here reads the box, so it is left folded — the state a pane comes up in.
      composeOpen: false,
      onFold: () => {},
      onFocus: () => {},
      }));
  });
}

const wayOut = () => container.querySelector<HTMLButtonElement>(".slot__end");
/** Press it, and let the asking and the ending it awaits settle. */
const press = async () => {
  await act(async () => { wayOut()?.click(); });
  await act(async () => { await Promise.resolve(); });
};

describe("removing a pane", () => {
  it("is offered on a place with nothing running in it", async () => {
    await pane(false);
    expect(wayOut(), "a place kept from the last run had no way to be got rid of").not.toBeNull();
  });

  it("asks first, and a refusal leaves the place and the terminal alone", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "/work/here", null);
    });
    hoisted.agrees = false;
    await press();

    expect(hoisted.asked, "the place was taken away without asking").toBe(1);
    expect(dropped, "a refusal took the place away anyway").toEqual([]);
    expect(hoisted.ended, "a refusal ended the terminal anyway").toEqual([]);
  });

  it("takes a second press while the first is still being answered as nothing", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "/work/here", null);
    });
    let answer = () => {};
    hoisted.held = new Promise((done) => { answer = done; });
    await press();
    await press();

    expect(hoisted.asked, "a press that piled up asked a second time").toBe(1);
    await act(async () => { answer(); });
    await act(async () => { await Promise.resolve(); });
    expect(dropped, "the place was taken away twice").toEqual(["1"]);
    expect(hoisted.ended).toEqual(["session-7"]);
  });

  it("ends the terminal that was running, then takes the place away", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "/work/here", null);
    });
    await press();

    expect(hoisted.ended, "the place went and left its session running").toEqual(["session-7"]);
    expect(dropped).toEqual(["1"]);
  });

  it("ends nothing where the program had already exited", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "/work/here", null);
    });
    await act(async () => { hoisted.events?.closed("session-7", 0); });
    await press();

    expect(hoisted.ended, "a session the host has forgotten was named anyway").toEqual([]);
    expect(dropped).toEqual(["1"]);
  });
});

describe("removing a launch's pane", () => {
  /** A run going, as a pane is handed its seat in front. */
  const GOING: Say = {
    automation: "Nightly", run: 31, step: "write the tests", automationId: 3, placement: 2, box: 2, builtin: false,
    interactive: false, by: null, command: null, startedAt: null, action: null, task: null,
    state: {
      status: "running", word: "Running", pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false,
      why: null, exit: null, errorExit: false, acknowledged: false,
    },
  };
  /** The launch 5, standing as `status`, with no roles drawn. */
  const launchOf = (status: AutomationLaunchStateDto["status"]): AutomationLaunchStateDto => ({
    launch: 5, project: 1, automation: 3, automationName: "Nightly", status, maxHeadcount: 10, maxRunLimit: 100,
    underWay: 0, roles: [], attention: [],
  } as AutomationLaunchStateDto);

  it("can be pressed while the seat in front is going, and asks whether to stop the launch", async () => {
    hoisted.launched = launchOf("running");
    await pane(false, { run: GOING, launch: 5 });
    expect(wayOut()?.disabled, "a launch's pane could not be taken away while its run went").toBe(false);
    hoisted.agrees = false;
    await press();

    expect(hoisted.questions, "the launch's pane was not asked about the launch").toEqual([t("face.dropLaunchConfirm")]);
    expect(hoisted.log, "a refusal stopped the launch or took the place away anyway").toEqual([]);
  });

  it("stops the launch on a yes, then takes the place away", async () => {
    hoisted.launched = launchOf("running");
    await pane(false, { run: GOING, launch: 5 });
    await press();

    expect(hoisted.log, "the place went and left the launch going").toEqual(["stop 5", "drop 1"]);
  });

  it("asks and stops a launch with no seat in front yet", async () => {
    hoisted.launched = launchOf("paused");
    await pane(false, { launch: 5 });
    await press();

    expect(hoisted.questions).toEqual([t("face.dropLaunchConfirm")]);
    expect(hoisted.log).toEqual(["stop 5", "drop 1"]);
  });

  it("keeps the place where the stop is refused", async () => {
    hoisted.launched = launchOf("running");
    hoisted.refuses = "no";
    await pane(false, { launch: 5 });
    await press();

    expect(dropped, "the place went though the launch it drew was still going").toEqual([]);
  });

  it("does not stop a launch that has ended, but still asks before the place goes", async () => {
    hoisted.launched = launchOf("ended");
    await pane(false, { launch: 5 });
    await press();

    expect(hoisted.questions).toEqual([t("face.dropConfirm")]);
    expect(hoisted.log, "an ended launch was stopped again").toEqual(["drop 1"]);
  });

  it("leaves a run's own pane outside a launch unpressable while its run is going", async () => {
    await pane(false, { run: GOING });
    expect(wayOut()?.disabled).toBe(true);
    await press();

    expect(hoisted.asked).toBe(0);
    expect(hoisted.log).toEqual([]);
  });
});
