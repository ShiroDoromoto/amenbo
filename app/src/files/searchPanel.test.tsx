// @vitest-environment jsdom
// Looking through the whole folder: what is asked of the host, what is drawn as the answer arrives,
// and what a press on a hit opens.
//
// The host is stood in for. What it does is walk a real folder with ten threads, which is not
// something a test of this screen is about — what is, is that the screen asks once per word rather
// than once per letter, draws each batch as it lands, and drops the answer to a word the reader has
// typed past.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FolderSearchDoneDto, FolderSearchFoundDto } from "../bindings/bindings";
import { errText, t, tf } from "../core/i18n";
import type { OpenFile } from "./FilesPanel";

const hoisted = vi.hoisted(() => ({
  /** Every search asked for, in order. */
  asked: [] as { root: string; query: string; ignored: boolean; caseSensitive: boolean; tag: number }[],
  /** Which searches were called off. */
  stopped: [] as number[],
  /** What the host would refuse the next ask with, or nothing. */
  refuse: null as unknown,
  /** Every replacement asked for, in order — the query with it, which the host reads groups by. */
  replaced: [] as { files: unknown[]; withText: string; asked: unknown }[],
  /** What the host answers a replacement with, or what it refuses one with. */
  wrote: { done: [], skipped: [] } as unknown,
  refuseWrite: null as unknown,
  /** The two listeners the screen puts on. */
  found: null as ((one: FolderSearchFoundDto) => void) | null,
  done: null as ((one: FolderSearchDoneDto) => void) | null,
}));

vi.mock("./folder", () => ({
  folderSearch: async (
    _projectId: number,
    root: string,
    ask: { query: string; ignored: boolean; caseSensitive: boolean; tag: number },
  ) => {
    hoisted.asked.push({ root, ...ask });
    if (hoisted.refuse !== null) throw hoisted.refuse;
  },
  folderSearchStop: async (tag: number) => { hoisted.stopped.push(tag); },
  folderReplace: async (
    _projectId: number,
    _root: string,
    files: unknown[],
    withText: string,
    asked: unknown,
  ) => {
    hoisted.replaced.push({ files, withText, asked });
    if (hoisted.refuseWrite !== null) throw hoisted.refuseWrite;
    return hoisted.wrote;
  },
  onFolderSearchFound: async (take: (one: FolderSearchFoundDto) => void) => {
    hoisted.found = take;
    return () => { hoisted.found = null; };
  },
  onFolderSearchDone: async (take: (one: FolderSearchDoneDto) => void) => {
    hoisted.done = take;
    return () => { hoisted.done = null; };
  },
}));

const { SearchPanel } = await import("./SearchPanel");

const ROOT = "/work/repo";
let container: HTMLElement;
let root: Root;

/** Draw the screen and let its listeners be put on. */
async function draw(onOpen: (at: OpenFile, line: number) => void = () => {}) {
  await act(async () => {
    root.render(<SearchPanel projectId={1} root={ROOT} onOpen={onOpen} />);
    await Promise.resolve();
  });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

/** Type into the one field, the way a keyboard does. */
async function type(text: string) {
  const field = container.querySelector<HTMLInputElement>(".search__field");
  if (field === null) throw new Error("no field");
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Let the wait before a walk go by, and whatever it asked for settle. */
async function walked() {
  await act(async () => { await new Promise((r) => setTimeout(r, 260)); });
}

/** One file's worth of an answer. */
const oneFile = (tag: number): FolderSearchFoundDto => ({
  root: ROOT,
  tag,
  files: [{
    path: ["src", "a.rs"],
    digest: "mark",
    more: false,
    lines: [{
      line: 12,
      from: 0,
      text: "one needle two",
      cut: false,
      spans: [{ at: 4, length: 6 }],
    }],
  }],
});

/** Two files' worth, which is what makes a replacement one the screen asks about. */
const twoFiles = (tag: number): FolderSearchFoundDto => ({
  ...oneFile(tag),
  files: [
    ...oneFile(tag).files,
    {
      path: ["b.txt"],
      digest: "other",
      more: false,
      lines: [{ line: 1, from: 0, text: "needle up top", cut: false, spans: [{ at: 0, length: 6 }] }],
    },
  ],
});

const over = (tag: number, some: Partial<FolderSearchDoneDto> = {}): FolderSearchDoneDto => ({
  root: ROOT, tag, files: 1, hits: 1, capped: false, stopped: false, ...some,
});

beforeEach(() => {
  hoisted.asked = [];
  hoisted.stopped = [];
  hoisted.refuse = null;
  hoisted.replaced = [];
  hoisted.wrote = { done: [], skipped: [] };
  hoisted.refuseWrite = null;
  hoisted.found = null;
  hoisted.done = null;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("looking through the whole folder", () => {
  it("walks the word rather than each letter on the way to it", async () => {
    await draw();
    await type("n");
    await type("ne");
    await type("needle");
    expect(hoisted.asked).toEqual([]);

    await walked();
    expect(hoisted.asked).toHaveLength(1);
    expect(hoisted.asked[0]?.query).toBe("needle");
    expect(hoisted.asked[0]?.root).toBe(ROOT);
  });

  it("draws each batch as it lands, and says what it came to", async () => {
    await draw();
    await type("needle");
    await walked();
    const tag = hoisted.asked[0]?.tag ?? 0;

    await act(async () => { hoisted.found?.(oneFile(tag)); });
    expect(container.querySelector(".search__path")?.textContent).toBe("src/a.rs");
    expect(container.querySelector(".search__lineno")?.textContent).toBe("12");
    // The match is marked inside the line, at the column the host counted.
    expect(container.querySelector(".search__mark")?.textContent).toBe("needle");

    await act(async () => { hoisted.done?.(over(tag)); });
    expect(container.textContent).toContain(tf("files.searchFound", { hits: 1, files: 1 }));
  });

  it("drops a batch from a search the reader has typed past", async () => {
    await draw();
    await type("needle");
    await walked();
    const tag = hoisted.asked[0]?.tag ?? 0;

    await type("pin");
    await walked();
    // The older walk answering last: its rows are about a word nobody is looking at any more.
    await act(async () => { hoisted.found?.(oneFile(tag)); });
    expect(container.querySelector(".search__path")).toBeNull();
  });

  it("opens the file at the line a hit is on", async () => {
    const opened: { root: string; path: string[]; line: number }[] = [];
    await draw((at, line) => { opened.push({ ...at, line }); });
    await type("needle");
    await walked();
    await act(async () => { hoisted.found?.(oneFile(hoisted.asked[0]?.tag ?? 0)); });

    await act(async () => {
      container.querySelector<HTMLButtonElement>(".search__hit")?.click();
    });
    expect(opened).toEqual([{ root: ROOT, path: ["src", "a.rs"], line: 12 }]);
  });

  it("says when nothing holds it, and when there is more than it drew", async () => {
    await draw();
    await type("zzz");
    await walked();
    const tag = hoisted.asked[0]?.tag ?? 0;
    await act(async () => { hoisted.done?.(over(tag, { files: 0, hits: 0 })); });
    expect(container.textContent).toContain(t("files.searchNone"));

    await type("e");
    await walked();
    const next = hoisted.asked[1]?.tag ?? 0;
    await act(async () => { hoisted.found?.(oneFile(next)); });
    await act(async () => { hoisted.done?.(over(next, { capped: true })); });
    expect(container.textContent).toContain(t("files.searchCapped"));
  });

  it("says the pattern is not one, rather than answering with nothing", async () => {
    hoisted.refuse = { code: "folder.search_pattern", message_en: "this is not a pattern" };
    await draw();
    await type("(");
    await walked();
    expect(container.textContent).toContain("this is not a pattern");
    expect(container.textContent).not.toContain(t("files.searchNone"));
  });

  it("says the tree draws what this leaves out, until the reader asks for it", async () => {
    await draw();
    expect(container.textContent).toContain(t("files.searchTreeNote"));

    await act(async () => {
      const box = container.querySelector<HTMLInputElement>(".search__ignored input");
      if (box === null) throw new Error("no switch");
      box.click();
    });
    expect(container.textContent).not.toContain(t("files.searchTreeNote"));
    await walked();
  });

  it("hides what it would write until the reader asks for it", async () => {
    await draw();
    expect(container.querySelector(".search__does")).toBeNull();

    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-expanded="false"]')?.click();
    });
    expect(container.querySelector(".search__does")).not.toBeNull();
    // Said where it is about to happen: the question below is not asked at all for one file.
    expect(container.textContent).toContain(t("files.searchNoUndo"));
  });

  it("writes only what is left in the list, and asks first where that is more than one file",
    async () => {
      await draw();
      await type("needle");
      await walked();
      const tag = hoisted.asked[0]?.tag ?? 0;
      await act(async () => { hoisted.found?.(twoFiles(tag)); });
      await act(async () => {
        container.querySelector<HTMLButtonElement>('[aria-expanded="false"]')?.click();
      });

      // Two files, so the press asks rather than writing.
      await act(async () => { container.querySelector<HTMLButtonElement>(".search__does")?.click(); });
      expect(document.body.querySelector('[role="dialog"]')).not.toBeNull();
      expect(hoisted.replaced).toEqual([]);

      await act(async () => {
        [...document.body.querySelectorAll<HTMLButtonElement>(".trashask__action")][0]?.click();
      });
      expect(hoisted.replaced).toHaveLength(1);
      expect(hoisted.replaced[0]?.files).toHaveLength(2);
    });

  it("sends the query down with the replacement, so the host can read a group out of it", async () => {
    await draw();
    // The switch that says the query is a pattern. Turning it on is what makes `$1` mean anything,
    // and the host is the only side that can read it — a column says where a match was, not what
    // was in its brackets.
    await act(async () => {
      [...container.querySelectorAll<HTMLButtonElement>(".search__switch")]
        .find((one) => one.textContent === ".*")
        ?.click();
    });
    await type("(need)le");
    await walked();
    await act(async () => { hoisted.found?.(oneFile(hoisted.asked[hoisted.asked.length - 1]?.tag ?? 0)); });
    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-expanded="false"]')?.click();
    });

    const fields = container.querySelectorAll<HTMLInputElement>(".search__field");
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(fields[1], "<$1>");
      fields[1]?.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => { container.querySelector<HTMLButtonElement>(".search__does")?.click(); });

    expect(hoisted.replaced).toHaveLength(1);
    expect(hoisted.replaced[0]?.withText).toBe("<$1>");
    // The same query the hits came from. Anything else and the host finds no match where they are.
    expect(hoisted.replaced[0]?.asked).toMatchObject({ query: "(need)le", regex: true });
  });

  it("leaves out what the reader took out, and stops asking once that is one file", async () => {
    await draw();
    await type("needle");
    await walked();
    await act(async () => { hoisted.found?.(twoFiles(hoisted.asked[0]?.tag ?? 0)); });
    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-expanded="false"]')?.click();
    });

    // The whole of the second file, taken out of the list.
    await act(async () => {
      [...container.querySelectorAll<HTMLButtonElement>(".search__head .search__drop")][1]?.click();
    });
    expect(container.querySelectorAll(".search__file")).toHaveLength(1);

    // One file now, so the press writes without asking.
    await act(async () => { container.querySelector<HTMLButtonElement>(".search__does")?.click(); });
    expect(document.body.querySelector('[role="dialog"]')).toBeNull();
    expect(hoisted.replaced).toHaveLength(1);
    expect(hoisted.replaced[0]?.files).toHaveLength(1);
  });

  it("says what a run came to, and goes on saying it while the list is asked again", async () => {
    hoisted.wrote = {
      done: [{ path: ["src", "a.rs"], hits: 1, digest: "after" }],
      skipped: [{ path: ["b.txt"], why: "changed" }],
    };
    await draw();
    await type("needle");
    await walked();
    await act(async () => { hoisted.found?.(twoFiles(hoisted.asked[0]?.tag ?? 0)); });
    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-expanded="false"]')?.click();
    });
    await act(async () => { container.querySelector<HTMLButtonElement>(".search__does")?.click(); });
    await act(async () => {
      [...document.body.querySelectorAll<HTMLButtonElement>(".trashask__action")][0]?.click();
    });

    expect(container.textContent).toContain(tf("files.searchReplaced", { hits: 1, files: 1 }));
    expect(container.textContent).toContain(tf("files.searchLeftAlone", { files: 1 }));
    // The run asks for the word again over what is there now, and what it came to is still said:
    // a line gone before it was read is a line nobody was told.
    await walked();
    expect(container.textContent).toContain(tf("files.searchReplaced", { hits: 1, files: 1 }));
  });

  it("says one file nothing may write to stopped every file in the run", async () => {
    // The refusal the host makes when one file of a run cannot be written to. It carries a code,
    // so what a reader is shown is this layer's own sentence for it rather than the English one.
    hoisted.refuseWrite = {
      code: "folder_replace_read_only",
      message_en: "2 of these files cannot be written to",
      fields: { count: 2, paths: ["src/a.rs", "b.txt"] },
    };
    await draw();
    await type("needle");
    await walked();
    await act(async () => { hoisted.found?.(twoFiles(hoisted.asked[0]?.tag ?? 0)); });
    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-expanded="false"]')?.click();
    });
    await act(async () => { container.querySelector<HTMLButtonElement>(".search__does")?.click(); });
    await act(async () => {
      [...document.body.querySelectorAll<HTMLButtonElement>(".trashask__action")][0]?.click();
    });
    expect(container.textContent).toContain(errText(hoisted.refuseWrite));
    expect(container.querySelector(".search__none")).not.toBeNull();
  });

  it("calls the walk off when the reader leaves the screen", async () => {
    await draw();
    await type("needle");
    await walked();
    const tag = hoisted.asked[0]?.tag ?? 0;

    await act(async () => { root.unmount(); });
    expect(hoisted.stopped).toContain(tag);
    // Drawn again so the teardown after this test has something to unmount.
    root = createRoot(container);
  });
});
