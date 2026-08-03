// @vitest-environment jsdom
// The read the board's ordering rests on (`AMB-D-533`): what a project is bound to, and — the half that
// decides whether anything is drawn at all — whether the answer is in yet.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createElement } from "react";
import type { BoundFolderDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What core answers with, or the rejection it answers with instead. */
  folders: [] as BoundFolderDto[],
  fails: false,
  /** Which projects were asked about, in order — evidence it is read per project, and once. */
  asked: [] as number[],
}));

vi.mock("./mutations", () => ({
  fetchBoundFolders: (projectId: number) => {
    hoisted.asked.push(projectId);
    return hoisted.fails ? Promise.reject(new Error("no")) : Promise.resolve(hoisted.folders);
  },
}));

import { useBoundFolders, type BoundFolders } from "./boundFolders";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const folder = (path: string, exists: boolean): BoundFolderDto =>
  ({ path, exists, mismatch: null, legacy: false, pointerMissing: false });

let container: HTMLDivElement;
let root: Root;
let seen: BoundFolders;

function Probe({ projectId }: { projectId: number }) {
  seen = useBoundFolders(projectId);
  return null;
}

const render = (projectId = 7) =>
  act(async () => { root.render(createElement(Probe, { projectId })); });

beforeEach(() => {
  hoisted.folders = [];
  hoisted.fails = false;
  hoisted.asked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the folders a project is bound to", () => {
  it("asks about the project on screen, and once", async () => {
    await render(7);

    expect(hoisted.asked).toEqual([7]);
    expect(seen.answered).toBe(true);
  });

  // A folder that has moved away is no folder an AI can be started in — the whole reason `live` is not
  // just `all`.
  it("keeps what is recorded apart from what is actually there", async () => {
    hoisted.folders = [folder("/w/here", true), folder("/w/gone", false)];
    await render();

    expect(seen.all.map((one) => one.path)).toEqual(["/w/here", "/w/gone"]);
    expect(seen.live.map((one) => one.path)).toEqual(["/w/here"]);
  });

  // The warning drawn from this says a project has no folder. A read that could not be made must not be
  // reported as an answer of none, so nothing is answered until it comes back.
  it("is unanswered until the read lands", async () => {
    let release: (rows: BoundFolderDto[]) => void = () => {};
    const pending = new Promise<BoundFolderDto[]>((r) => { release = r; });
    const mutations = await import("./mutations");
    vi.spyOn(mutations, "fetchBoundFolders").mockReturnValueOnce(pending as never);

    await render();
    expect(seen.answered, "nothing to say while the read is out").toBe(false);

    await act(async () => { release([folder("/w/here", true)]); });
    expect(seen.answered).toBe(true);
    expect(seen.live).toHaveLength(1);
  });

  // Being unable to read is not being able to say there is a folder. The invitation to link one is the
  // safe half of being wrong, so a failure answers with none rather than staying silent for ever.
  it("answers with none when the read fails", async () => {
    hoisted.fails = true;
    await render();

    expect(seen.answered).toBe(true);
    expect(seen.all).toEqual([]);
  });
});
