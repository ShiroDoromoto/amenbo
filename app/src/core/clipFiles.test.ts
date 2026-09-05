// @vitest-environment jsdom
// What a paste carrying files is answered with, and what is left alone.
//
// The reading is narrow on purpose: a paste of ordinary words belongs to whatever box it landed in,
// which knows things this does not (`./clipFiles`). What is answered is the other one — the paste a
// window cannot read a path out of — and the whole of the answer is that the host is asked and the
// caller is handed what came back.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { holdsFiles, takesPastedFiles } from "./clipFiles";

const hoisted = vi.hoisted(() => ({
  /** What the host says is on the clipboard, for a test to set. */
  paths: [] as string[],
  /** Whether the host refuses to answer at all. */
  refuses: false,
  /** The commands that were asked. */
  asked: [] as string[],
}));

vi.mock("./ipc", () => ({
  invoke: vi.fn(async (cmd: string) => {
    hoisted.asked.push(cmd);
    if (hoisted.refuses) throw new Error("the clipboard would not answer");
    return hoisted.paths;
  }),
}));

/** A paste, with the three things this reads held down to what a test gives it. */
function carrying(over: { files?: number; types?: string[]; words?: string } = {}): DataTransfer {
  return {
    files: { length: over.files ?? 0 },
    types: over.types ?? [],
    getData: (type: string) => (type === "text/plain" ? (over.words ?? "") : ""),
  } as unknown as DataTransfer;
}

/** The paste event, on the box inside the host — which is where a real one lands. */
function paste(box: HTMLElement, data: DataTransfer | null): ClipboardEvent {
  const e = new Event("paste", { bubbles: true, cancelable: true }) as ClipboardEvent;
  Object.defineProperty(e, "clipboardData", { value: data });
  box.dispatchEvent(e);
  return e;
}

describe("telling a paste carrying files from a paste of words", () => {
  it("is one carrying files, said either way round", () => {
    expect(holdsFiles(carrying({ files: 1 })), "the files themselves").toBe(true);
    expect(holdsFiles(carrying({ types: ["Files"] })), "what the paste says it holds").toBe(true);
    expect(holdsFiles(carrying({ files: 2, types: ["Files", "text/plain"] }))).toBe(true);
  });

  it("is not a paste of words, however much of it there is", () => {
    expect(holdsFiles(carrying({ types: ["text/plain"] }))).toBe(false);
    expect(holdsFiles(carrying({ types: ["text/plain", "text/html"] }))).toBe(false);
    expect(holdsFiles(carrying())).toBe(false);
  });
});

describe("answering the pastes a box is given", () => {
  let host: HTMLElement;
  let box: HTMLElement;
  let put: ReturnType<typeof vi.fn>;
  let stop: () => void;

  beforeEach(() => {
    hoisted.paths = [];
    hoisted.refuses = false;
    hoisted.asked = [];
    host = document.createElement("div");
    box = document.createElement("div");
    host.append(box);
    document.body.append(host);
    put = vi.fn();
    stop = takesPastedFiles(host, put as (paths: string[], words: string) => void);
  });

  it("hands over the paths the host read back", async () => {
    hoisted.paths = ["/work/a plain one.md", "/work/100% done.md"];

    paste(box, carrying({ files: 2, types: ["Files"] }));
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    expect(put).toHaveBeenCalledWith(["/work/a plain one.md", "/work/100% done.md"], "");
  });

  // The paste is taken away from the box before it can answer it: the box's own listener sits on the
  // box, and this one is on the way down to it. Both halves are what stop it — one keeps the box out
  // of it, the other keeps the page from answering the paste itself.
  it("takes such a paste away from the box it landed in", () => {
    const box_heard = vi.fn();
    box.addEventListener("paste", box_heard);

    const e = paste(box, carrying({ files: 1, types: ["Files"] }));

    expect(e.defaultPrevented, "the page was left to answer it").toBe(true);
    expect(box_heard, "the box answered it as well").not.toHaveBeenCalled();
  });

  // What the paste itself carried, for a clipboard holding files this machine will not name — a file
  // manager that puts a file on and no path with it (`AMB-T-4220`).
  it("falls back to the words the paste carried, where the host names no file", async () => {
    hoisted.paths = [];

    paste(box, carrying({ files: 1, types: ["Files"], words: "/work/named.md" }));
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    expect(put).toHaveBeenCalledWith([], "/work/named.md");
  });

  it("leaves a paste of ordinary words where it landed", () => {
    const e = paste(box, carrying({ types: ["text/plain"], words: "just some words" }));

    expect(e.defaultPrevented, "the box was not left to answer it").toBe(false);
    expect(hoisted.asked, "the host was asked about a paste with no file on it").toEqual([]);
  });

  it("leaves a paste carrying nothing at all where it landed", () => {
    const e = paste(box, null);

    expect(e.defaultPrevented).toBe(false);
    expect(hoisted.asked).toEqual([]);
  });

  // A host that will not answer leaves the box with nothing in it, which is what it had before —
  // rather than an error nobody in a text box can act on.
  it("says nothing where the host will not answer", async () => {
    hoisted.refuses = true;

    paste(box, carrying({ files: 1, types: ["Files"] }));
    await vi.waitFor(() => expect(hoisted.asked).toEqual(["clip_files"]));

    expect(put).not.toHaveBeenCalled();
  });

  it("stops listening when it is told to", () => {
    stop();

    const e = paste(box, carrying({ files: 1, types: ["Files"] }));

    expect(e.defaultPrevented).toBe(false);
    expect(hoisted.asked).toEqual([]);
  });
});
