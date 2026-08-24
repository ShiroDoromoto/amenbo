// @vitest-environment jsdom
// What the terminal face says about work nothing is doing any more, and the two things it must not do:
// ask before there is a project to ask about, and act on the answer.
//
// The host's read is stubbed and everything else runs — the point of the component is entirely in
// which of the two shapes it draws and what pressing them does.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AdriftDto, AdriftRowDto } from "../bindings/bindings";
import { RefNavProvider } from "../core/refNav";
import { AdriftSlot } from "./AdriftSlot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the host answers with, and which folders it was asked about. */
  adrift: { tasks: [], decisions: [] } as unknown,
  asked: [] as string[],
}));

vi.mock("../core/mutations", () => ({
  fetchAdrift: vi.fn(async (folder: string) => {
    hoisted.asked.push(folder);
    return hoisted.adrift;
  }),
}));

/** A row, as the host hands one over. */
function row(ref: string, id: number, title: string): AdriftRowDto {
  return { id, ref, title };
}

/** What the host answers with, with the half a test does not care about empty. */
function left(over: Partial<AdriftDto>): AdriftDto {
  return { tasks: [], decisions: [], ...over };
}

let container: HTMLDivElement;
let root: Root;
const opened: string[] = [];
const ledger: number[] = [];

beforeEach(() => {
  hoisted.adrift = left({});
  hoisted.asked = [];
  opened.length = 0;
  ledger.length = 0;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Draw it, with the way to the ledger and the way to a task both counted. `wayIn` is whether it
 *  stands on its own where there is nothing to ask — a face with no panes on it. */
async function draw(folder: string | null, wayIn = true): Promise<void> {
  await act(async () => {
    root.render(
      createElement(
        RefNavProvider,
        {
          value: {
            selectTask: (id: number) => { opened.push(`task ${id}`); },
            selectDecision: (id: number | null) => { opened.push(`decision ${id}`); },
          },
          children: createElement(AdriftSlot, {
            folder,
            wayIn,
            onOpen: () => {},
            onOpenLedger: () => ledger.push(1),
          }),
        },
      ),
    );
  });
}

function buttons(): HTMLButtonElement[] {
  return [...container.querySelectorAll("button")];
}

describe("a face with nothing to ask about", () => {
  it("draws nothing at all beside panes that are open", async () => {
    await draw("/work/here", false);
    // An empty box beside a terminal is the identical question this face was built to stop asking.
    expect(container.textContent).toBe("");
  });

  it("is the plain way to open a terminal where nothing is open", async () => {
    await draw("/work/here");

    expect(buttons()).toHaveLength(1);
    expect(container.querySelector(".slot--empty"), "the plain slot was not drawn").toBeTruthy();
  });

  it("asks the host nothing about a page that has no folder yet", async () => {
    await draw(null);

    expect(hoisted.asked, "a page with no project was asked about anyway").toEqual([]);
    expect(container.querySelector(".slot--empty")).toBeTruthy();
  });
});

describe("a face with something left in the middle", () => {
  it("puts one question over both kinds, and keeps the way to open a terminal", async () => {
    hoisted.adrift = left({
      tasks: [row("AMB-T-11", 11, "the migration")],
      decisions: [row("AMB-D-21", 21, "which of the two roads")],
    });
    await draw("/work/here");

    expect(hoisted.asked).toEqual(["/work/here"]);
    // One sentence, not one per kind: it is one question, and the ref on each row says which kind it
    // is and so what pressing it opens.
    expect(container.querySelectorAll(".adrift__ask")).toHaveLength(1);
    expect(container.textContent).toContain("Carry on with it?");
    expect(container.textContent).toContain("the migration");
    expect(container.textContent).toContain("which of the two roads");
    // The way in is still there: the face is somewhere to start a terminal whether or not there
    // is anything to be asked about.
    expect(buttons().some((b) => b.textContent === "Open a terminal here")).toBe(true);
  });

  it("opens a task on the task face and a decision on the decision face, and moves neither", async () => {
    hoisted.adrift = left({
      tasks: [row("AMB-T-11", 11, "the migration")],
      decisions: [row("AMB-D-21", 21, "which of the two roads")],
    });
    await draw("/work/here");

    const press = async (words: string) => {
      const one = buttons().find((b) => b.textContent?.includes(words));
      expect(one, `"${words}" was not pressable`).toBeTruthy();
      await act(async () => { one?.click(); });
    };
    await press("the migration");
    await press("which of the two roads");

    // The ledger first each time — a press that selected without switching would land on a face the
    // reader cannot see — and each kind on the face that reads it.
    expect(ledger, "the ledger was not brought up").toEqual([1, 1]);
    expect(opened).toEqual(["task 11", "decision 21"]);
  });

  it("asks about a decision alone, where that is all there is", async () => {
    hoisted.adrift = left({ decisions: [row("AMB-D-21", 21, "which of the two roads")] });
    await draw("/work/here");

    expect(container.querySelector(".slot--adrift"), "the plain slot was drawn").toBeTruthy();
    expect(container.textContent).toContain("which of the two roads");
  });
});
