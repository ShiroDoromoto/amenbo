// @vitest-environment jsdom
// Unit-testing the pure logic on its own (edgeRows / promotesToAccepted / standingOn in `core/decisionEdges.ts`)
// proves nothing about the pane actually using it: promoting without warning first, or unlinking the wrong edge
// from a back-reference row, would both slip through. What is checked here is the wiring to the pane.
//
// The edge controls are wrapped in `inTauri()` and so are not drawn in bare jsdom — hence we claim to be inside
// the Tauri shell. Only the boundaries are replaced (reads, writes, the native confirm dialog, attachments); the
// pane's rendering and branching are the real thing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Decision, DecisionRef } from "../core/snapshot";

const hoisted = vi.hoisted(() => ({
  /** The decisions the detail pane reads (id → decision). The picker's candidates are built from the same set. */
  decisions: new Map<number, Decision>(),
  /** The picker's candidates (`useDecisionPage`). */
  page: [] as Decision[],
  /** The wording handed to `confirmDialog`, in order. */
  asked: [] as string[],
  /** The confirm dialog's answers, consumed from the front; once exhausted, everything is an OK. */
  answers: [] as boolean[],
  /** The writes that were called, arguments and all. */
  calls: [] as Array<Array<number | string>>,
  /** Names of writes that reject instead of resolving (how the store refusing a write looks from here). */
  failing: new Set<string>(),
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true };
});
vi.mock("../core/reads", () => ({
  useDecision: (id: number) => hoisted.decisions.get(id),
  useDecisionComments: () => [],
  useDecisionPage: () => hoisted.page,
}));
vi.mock("../core/dialog", () => ({
  confirmDialog: (message: string) => {
    hoisted.asked.push(message);
    return Promise.resolve(hoisted.answers.shift() ?? true);
  },
}));
vi.mock("../core/mutations", () => {
  const record = (name: string) => (...args: Array<number | string>) => {
    hoisted.calls.push([name, ...args]);
    if (hoisted.failing.has(name)) return Promise.reject(new Error(`${name} refused`));
    return Promise.resolve();
  };
  return {
    acceptDecision: record("acceptDecision"), rejectDecision: record("rejectDecision"),
    reopenDecision: record("reopenDecision"), supersedeDecision: record("supersedeDecision"),
    amendDecision: record("amendDecision"), buildsOnDecision: record("buildsOnDecision"),
    unlinkDecisionEdge: record("unlinkDecisionEdge"), addDecisionComment: record("addDecisionComment"),
    editDecisionComment: record("editDecisionComment"), removeDecisionComment: record("removeDecisionComment"),
  };
});
// The attachment list invokes the store, which is none of this pane's business.
vi.mock("../components/Attachments", () => ({ Attachments: () => null }));

import { DecisionDetailPane } from "./DecisionDetailPane";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const opened: number[] = [];

/** One decision. By default: proposed, no edges, under a project — the conditions under which the picker shows. */
function decision(id: number, over: Partial<Decision> = {}): Decision {
  return {
    id, ref: `D-${id}`, title: `決定${id}`, body: "", status: "proposed", current: true,
    project: { id: 1, name: "検証PJ" },
    supersedes: [], supersededBy: [], amends: [], amendedBy: [], buildsOn: [], builtOnBy: [],
    decidedAt: null, decidedBy: null, linkedTasks: [], createdAt: "2026-07-12T00:00:00Z",
    ...over,
  } as Decision;
}
const ref = (id: number): DecisionRef => ({ id, name: `決定${id}`, ref: `D-${id}` });

/** Draw the pane. It looks the decision up by id in `decisions`. */
function render(decisionId: number) {
  act(() =>
    root.render(createElement(DecisionDetailPane, {
      decisionId,
      onOpenDecision: (id: number) => opened.push(id),
    })),
  );
}

/** Flush an operation that awaits the confirm dialog on its way through. */
async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

const buttons = () => Array.from(container.querySelectorAll("button"));
const button = (text: string) => buttons().find((b) => b.textContent?.includes(text));
const click = (b: Element | undefined) => act(() => (b as HTMLButtonElement).click());
/** Type into a controlled field: React listens for the native setter, so assigning `.value` alone is not seen. */
function type(el: HTMLTextAreaElement, text: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  act(() => {
    set.call(el, text);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  hoisted.decisions.clear();
  hoisted.page.length = 0;
  hoisted.asked.length = 0;
  hoisted.answers.length = 0;
  hoisted.calls.length = 0;
  hoisted.failing.clear();
  opened.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("edge rows (navigation and unlink)", () => {
  // Give it one forward edge (drawn by this decision) and one back-reference (drawn by the other). Only the
  // back-reference row reverses the direction of the unlink, and getting that wrong unlinks a different edge.
  const d = decision(1, { buildsOn: [{ ...ref(2) }], supersededBy: [ref(3)] });

  beforeEach(() => {
    hoisted.decisions.set(1, d);
    render(1);
  });

  it("clicking an edge's other end opens that decision (forward and back-reference alike)", () => {
    click(button("D-2"));
    click(button("D-3"));
    expect(opened).toEqual([2, 3]);
  });

  it("unlinking prompts for confirmation, and on OK removes it in the drawn direction (newer → older)", async () => {
    // Forward (this decision builds on `D-2`): from=1, to=2.
    click(buttons().filter((b) => b.textContent === t("dec.edge.unlink"))[0]);
    await settle();
    expect(hoisted.asked[0]).toBe(tf("dec.edge.unlinkConfirm", { target: "D-2" }));
    expect(hoisted.calls).toEqual([["unlinkDecisionEdge", 1, 2]]);
  });

  it("a back-reference row reverses the direction (unlinking the edge the other side drew)", async () => {
    // The back-reference (`D-3` superseded this decision): from=3, to=1. Unlinking 1 → 3 here would hit nothing.
    click(buttons().filter((b) => b.textContent === t("dec.edge.unlink"))[1]);
    await settle();
    expect(hoisted.calls).toEqual([["unlinkDecisionEdge", 3, 1]]);
  });

  it("does not unlink on Cancel", async () => {
    hoisted.answers.push(false);
    click(buttons().filter((b) => b.textContent === t("dec.edge.unlink"))[0]);
    await settle();
    expect(hoisted.calls).toEqual([]);
  });
});

describe("drawing an edge (promotion warning and blast radius)", () => {
  /** Open the picker and choose a kind of edge. */
  function openPicker(kind: string) {
    click(button(t("dec.edge.add")));
    const select = container.querySelector("select")!;
    act(() => {
      select.value = kind;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
  }
  const pick = (targetRef: string) => click(buttons().find((b) => b.textContent?.startsWith(targetRef)));

  it("under-discussion × \"supersede this\" warns first, and on Cancel does not supersede (nor promote)", async () => {
    hoisted.decisions.set(1, decision(1));
    hoisted.page.push(decision(2));
    render(1);

    openPicker("supersedes");
    expect(container.textContent).toContain(t("dec.edge.supersedeAccepts")); // warned as soon as the kind is chosen

    hoisted.answers.push(false);
    pick("D-2");
    await settle();
    expect(hoisted.asked).toEqual([tf("dec.edge.supersedeAcceptsConfirm", { target: "D-2" })]);
    expect(hoisted.calls).toEqual([]);

    pick("D-2"); // this time, OK
    await settle();
    expect(hoisted.calls).toEqual([["supersedeDecision", 1, 2]]);
  });

  it("superseding from an accepted decision does not promote, so it gives no warning", async () => {
    hoisted.decisions.set(1, decision(1, { status: "accepted" }));
    hoisted.page.push(decision(2));
    render(1);

    openPicker("supersedes");
    expect(container.textContent).not.toContain(t("dec.edge.supersedeAccepts"));

    pick("D-2");
    await settle();
    expect(hoisted.asked).toEqual([]);
    expect(hoisted.calls).toEqual([["supersedeDecision", 1, 2]]);
  });

  it("amends and builds-on are drawn directly, with no confirmation", async () => {
    hoisted.decisions.set(1, decision(1));
    hoisted.page.push(decision(2, { builtOnBy: [ref(9)] })); // a blast radius alone warns nobody unless it is a supersede
    render(1);

    openPicker("amends");
    pick("D-2");
    await settle();
    expect(hoisted.asked).toEqual([]);
    expect(hoisted.calls).toEqual([["amendDecision", 1, 2]]);

    openPicker("buildsOn");
    pick("D-2");
    await settle();
    expect(hoisted.calls[1]).toEqual(["buildsOnDecision", 1, 2]);
  });

  it("when decisions stand on the target, it follows the promotion warning with a revisit prompt (Cancel does not supersede)", async () => {
    hoisted.decisions.set(1, decision(1));
    hoisted.page.push(decision(2, { builtOnBy: [ref(9)] }));
    render(1);

    openPicker("supersedes");
    hoisted.answers.push(true, false); // OK to the promotion warning, then back out at the revisit confirm
    pick("D-2");
    await settle();
    expect(hoisted.asked).toEqual([
      tf("dec.edge.supersedeAcceptsConfirm", { target: "D-2" }),
      tf("dec.edge.supersedeRevisitConfirm", { target: "D-2", list: "D-9 決定9" }),
    ]);
    expect(hoisted.calls).toEqual([]);
  });
});

describe("rejection blast radius", () => {
  it("lists the decisions that stand on it, opens one when clicked, and never blocks the rejection itself", async () => {
    hoisted.decisions.set(1, decision(1, { supersededBy: [ref(7)], builtOnBy: [ref(8)] }));
    render(1);

    click(button(t("dec.reject")));
    expect(container.textContent).toContain(t("dec.revisit"));

    opened.length = 0; // the edge rows carry buttons for the same decisions, so take only the revisit list's rows
    const list = container.querySelector(".compose")!;
    const rows = Array.from(list.querySelectorAll("button")).filter((b) => b.textContent?.startsWith("D-"));
    expect(rows.map((b) => b.textContent?.trim())).toEqual(["D-7 決定7", "D-8 決定8"]);
    click(rows[0]);
    expect(opened).toEqual([7]);

    // It only shows them; the rejection itself is never blocked.
    click(button(t("dec.reject")));
    await settle();
    expect(hoisted.calls).toEqual([["rejectDecision", 1, ""]]);
  });

  it("shows nothing when no decision stands on it", () => {
    hoisted.decisions.set(1, decision(1));
    render(1);

    click(button(t("dec.reject")));
    expect(container.textContent).not.toContain(t("dec.revisit"));
  });

  it("accepting overturns nothing, so it shows no revisit prompt", () => {
    hoisted.decisions.set(1, decision(1, { builtOnBy: [ref(8)] }));
    render(1);

    click(button(t("dec.accept")));
    expect(container.textContent).not.toContain(t("dec.revisit"));
  });
});

// A write that never lands must never read as one that did. The pane used to drop the promise and close the
// panel regardless, so a refused accept looked exactly like a successful one — the badge simply stayed put.
describe("a refused accept/reject is shown, not swallowed", () => {
  const open = async (which: "accept" | "reject") => {
    click(button(t(`dec.${which}`)));
    click(button(t(`dec.${which}`))); // the green button in the confirm panel
    await settle();
  };

  it("keeps the confirm panel open and reports the error when the accept is refused", async () => {
    hoisted.failing.add("acceptDecision");
    hoisted.decisions.set(1, decision(1));
    render(1);

    await open("accept");
    expect(hoisted.calls).toEqual([["acceptDecision", 1, ""]]);
    expect(container.querySelector("[role=alert]")?.textContent).toContain("acceptDecision refused");
    expect(button(t("dec.cancel"))).toBeDefined(); // still confirming, so a retry costs nothing
  });

  it("keeps the reason the user typed when the write fails", async () => {
    hoisted.failing.add("rejectDecision");
    hoisted.decisions.set(1, decision(1));
    render(1);

    click(button(t("dec.reject")));
    type(container.querySelector("textarea")!, "根拠が薄い");
    click(button(t("dec.reject")));
    await settle();

    expect(hoisted.calls).toEqual([["rejectDecision", 1, "根拠が薄い"]]);
    expect(container.querySelector("textarea")?.value).toBe("根拠が薄い");
  });

  it("closes the panel once the write lands", async () => {
    hoisted.decisions.set(1, decision(1));
    render(1);

    await open("accept");
    expect(container.querySelector("[role=alert]")).toBeNull();
    expect(button(t("dec.cancel"))).toBeUndefined();
  });
});

// The same swallow lived on the rest of the pane's writes: a comment post that blanked the box on failure, a
// reopen/unlink/edge-wire that dropped the promise. Each must now await and report a refusal instead.
describe("the remaining pane writes report a refusal, not swallow it", () => {
  it("keeps the comment text and shows the error when the post is refused", async () => {
    hoisted.failing.add("addDecisionComment");
    hoisted.decisions.set(1, decision(1));
    render(1);

    type(container.querySelector("textarea")!, "本文");
    click(button(t("detail.send")));
    await settle();

    expect(hoisted.calls).toEqual([["addDecisionComment", 1, "本文"]]);
    expect(container.querySelector("[role=alert]")?.textContent).toContain("addDecisionComment refused");
    expect(container.querySelector("textarea")?.value).toBe("本文"); // the body survives for a retry
  });

  it("clears the comment box once the post lands", async () => {
    hoisted.decisions.set(1, decision(1));
    render(1);

    type(container.querySelector("textarea")!, "本文");
    click(button(t("detail.send")));
    await settle();

    expect(hoisted.calls).toEqual([["addDecisionComment", 1, "本文"]]);
    expect(container.querySelector("textarea")?.value).toBe("");
  });

  it("surfaces a refused reopen", async () => {
    hoisted.failing.add("reopenDecision");
    hoisted.decisions.set(1, decision(1, { status: "accepted" }));
    render(1);

    click(button(t("dec.reopen")));
    await settle();

    expect(hoisted.calls).toEqual([["reopenDecision", 1]]);
    expect(container.querySelector("[role=alert]")?.textContent).toContain("reopenDecision refused");
  });

  it("surfaces a refused edge unlink", async () => {
    hoisted.failing.add("unlinkDecisionEdge");
    hoisted.decisions.set(1, decision(1, { buildsOn: [{ ...ref(2) }] }));
    render(1);

    click(buttons().filter((b) => b.textContent === t("dec.edge.unlink"))[0]);
    await settle();

    expect(hoisted.calls).toEqual([["unlinkDecisionEdge", 1, 2]]);
    expect(container.querySelector("[role=alert]")?.textContent).toContain("unlinkDecisionEdge refused");
  });

  it("keeps the edge picker open and shows the error when wiring is refused", async () => {
    hoisted.failing.add("amendDecision");
    hoisted.decisions.set(1, decision(1));
    hoisted.page.push(decision(2));
    render(1);

    click(button(t("dec.edge.add")));
    const select = container.querySelector("select")!;
    act(() => { select.value = "amends"; select.dispatchEvent(new Event("change", { bubbles: true })); });
    click(buttons().find((b) => b.textContent?.startsWith("D-2")));
    await settle();

    expect(hoisted.calls).toEqual([["amendDecision", 1, 2]]);
    expect(container.querySelector("[role=alert]")?.textContent).toContain("amendDecision refused");
    expect(container.querySelector("select")).not.toBeNull(); // still open, so a retry costs nothing
  });
});
