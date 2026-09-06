// @vitest-environment jsdom
// What a paste carrying files is answered with, and what is left alone.
//
// The reading is narrow on purpose: a paste of ordinary words belongs to whatever box it landed in,
// which knows things this does not (`./clipFiles`). What is answered is the other one — the paste a
// window cannot read a path out of — and the whole of the answer is that the host is asked and the
// caller is handed what came back.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { holdsFiles, imageIn, takesPastedFiles, takesPastedImages } from "./clipFiles";

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
function carrying(
  over: { files?: number; image?: string; bytes?: string; types?: string[]; words?: string } = {},
): DataTransfer {
  const files: File[] = [];
  for (let i = 0; i < (over.files ?? 0); i++) {
    files.push(new File(["a note"], `note-${i}.md`, { type: "text/markdown" }));
  }
  if (over.image !== undefined) {
    files.push(new File([over.bytes ?? "the image"], "pasted", { type: over.image }));
  }
  return {
    files,
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

describe("finding the image a paste is carrying", () => {
  it("is the one file on it that is an image", () => {
    expect(imageIn(carrying({ image: "image/png" }))?.type).toBe("image/png");
    expect(imageIn(carrying({ files: 2, image: "image/jpeg" }))?.type).toBe("image/jpeg");
  });

  it("is none where nothing on it is", () => {
    expect(imageIn(carrying({ files: 2, types: ["Files"] }))).toBe(null);
    expect(imageIn(carrying())).toBe(null);
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

  // A screenshot is bytes and no file, so `clip_files` has nothing to name and the words are empty
  // too. What makes it pasteable is writing it down somewhere and handing back where that was.
  it("writes down the image a paste is carrying, where the host names no file", async () => {
    hoisted.paths = [];
    const written = vi.fn(async () => ["/tmp/amenbo-pasted-aa/pasted-1234abcd.png"]);
    stop();
    stop = takesPastedFiles(host, put as (paths: string[], words: string) => void, written);

    paste(box, carrying({ image: "image/png", bytes: "PNG", types: ["Files"] }));
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    const [bytes, mime] = written.mock.calls[0] as unknown as [Uint8Array, string];
    expect(new TextDecoder().decode(bytes), "the bytes the paste was carrying").toBe("PNG");
    expect(mime, "and the type they are in").toBe("image/png");
    expect(put).toHaveBeenCalledWith(["/tmp/amenbo-pasted-aa/pasted-1234abcd.png"], "");
  });

  // A file manager's copy of a `.png` carries the file *and* the path to it. The path is the file
  // itself; writing the bytes down again would hand the reader a copy of what they already have.
  it("takes the host's paths over the image, where there are both", async () => {
    hoisted.paths = ["/work/shot.png"];
    const written = vi.fn(async () => ["/tmp/never.png"]);
    stop();
    stop = takesPastedFiles(host, put as (paths: string[], words: string) => void, written);

    paste(box, carrying({ image: "image/png", types: ["Files"] }));
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    expect(written).not.toHaveBeenCalled();
    expect(put).toHaveBeenCalledWith(["/work/shot.png"], "");
  });

  // A caller that said nothing about images takes none: the paste is answered the way it was before
  // there was anywhere to put one.
  it("leaves the image alone where the caller has nowhere to put it", async () => {
    hoisted.paths = [];

    paste(box, carrying({ image: "image/png", types: ["Files"], words: "" }));
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    expect(put).toHaveBeenCalledWith([], "");
  });

  // An image that could not be written down leaves the reader with the words, which for an image is
  // nothing — rather than a path to a file that is not there.
  it("hands over no path where the image could not be written down", async () => {
    hoisted.paths = [];
    const written = vi.fn(async () => []);
    stop();
    stop = takesPastedFiles(host, put as (paths: string[], words: string) => void, written);

    paste(box, carrying({ image: "image/png", types: ["Files"] }));
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    expect(put).toHaveBeenCalledWith([], "");
  });

  it("stops listening when it is told to", () => {
    stop();

    const e = paste(box, carrying({ files: 1, types: ["Files"] }));

    expect(e.defaultPrevented).toBe(false);
    expect(hoisted.asked).toEqual([]);
  });
});

// Linux carries a pasted image on neither the paste nor the words: WebKitGTK leaves `clipboardData`
// empty (`AMB-T-4427`), so the press is read instead and the machine's clipboard is asked directly.
// The press is `Ctrl+Shift+V`, which is what pastes into a terminal; `Ctrl+V` is `^V` there.
describe("answering the press, where the paste says nothing", () => {
  let host: HTMLElement;
  let box: HTMLElement;
  let read: ReturnType<typeof vi.fn>;

  /** The machine's clipboard, holding what a test names. */
  function holding(...types: string[]): void {
    read = vi.fn(async () => [
      {
        types,
        getType: async (type: string) => new Blob(["the image"], { type }),
      },
    ]);
    Object.defineProperty(navigator, "clipboard", { value: { read }, configurable: true });
  }

  /** A press, on the box inside the host — which is where a real one lands. */
  function press(over: Partial<KeyboardEventInit> = {}): void {
    box.dispatchEvent(
      new KeyboardEvent("keydown", { key: "v", ctrlKey: true, shiftKey: true, bubbles: true, ...over }),
    );
  }

  beforeEach(() => {
    host = document.createElement("div");
    box = document.createElement("div");
    host.append(box);
    document.body.append(host);
    holding("image/png");
  });

  it("reads the image off the clipboard and writes it down", async () => {
    const written = vi.fn(async () => ["/tmp/amenbo-pasted-aa/pasted-1234abcd.png"]);
    const put = vi.fn();
    takesPastedImages(host, written, put, "other");

    press();
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    const [bytes, mime] = written.mock.calls[0] as unknown as [Uint8Array, string];
    expect(new TextDecoder().decode(bytes)).toBe("the image");
    expect(mime).toBe("image/png");
    expect(put).toHaveBeenCalledWith(["/tmp/amenbo-pasted-aa/pasted-1234abcd.png"]);
  });

  // The other two machines carry the image on the paste itself, which is already in hand there.
  it("does not ask the clipboard on the machines that carry the image on the paste", () => {
    for (const os of ["macos", "windows"] as const) {
      takesPastedImages(host, vi.fn(async () => []), vi.fn(), os);
    }

    press();

    expect(read).not.toHaveBeenCalled();
  });

  it("writes nothing down where the clipboard holds no image", async () => {
    holding("text/plain");
    const written = vi.fn(async () => ["/tmp/never.png"]);
    const put = vi.fn();
    takesPastedImages(host, written, put, "other");

    press();
    await vi.waitFor(() => expect(put).toHaveBeenCalled());

    expect(written).not.toHaveBeenCalled();
    expect(put).toHaveBeenCalledWith([]);
  });

  it("is that press and no other", () => {
    takesPastedImages(host, vi.fn(async () => []), vi.fn(), "other");

    press({ ctrlKey: false });
    // `Ctrl+V` without shift is `^V` to the program, and a path pasted after one arrives quoted.
    press({ shiftKey: false });
    press({ altKey: true });
    press({ key: "c" });

    expect(read).not.toHaveBeenCalled();
  });

  it("stops listening when it is told to", () => {
    const stop = takesPastedImages(host, vi.fn(async () => []), vi.fn(), "other");

    stop();
    press();

    expect(read).not.toHaveBeenCalled();
  });

  // A clipboard that will not answer — a permission that was not given, a version that refuses the
  // call — leaves the press where it was rather than an error nobody in a pane can act on.
  it("says nothing where the clipboard will not answer", async () => {
    read = vi.fn(async () => {
      throw new Error("NotAllowedError");
    });
    Object.defineProperty(navigator, "clipboard", { value: { read }, configurable: true });
    const put = vi.fn();
    takesPastedImages(host, vi.fn(async () => []), put, "other");

    press();
    await vi.waitFor(() => expect(read).toHaveBeenCalled());

    expect(put).not.toHaveBeenCalled();
  });
});
