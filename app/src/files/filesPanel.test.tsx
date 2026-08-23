// @vitest-environment jsdom
// What the file face has to get right, none of which is visible in the markup on its own.
//
// It is rooted at the **project's** folder: the rows are asked for with the folder the project is
// bound to, so a pane switching underneath them changes nothing (`AMB-T-3602`). It reads a file by
// asking the host what the file is, rather than deciding from the name — the panel draws Markdown
// as Markdown, and says plainly when there is nothing it can show. And the tree stays folded until
// somebody opens it, one level per opening, because a panel nobody is looking at must not walk a
// repository.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FolderChangedDto, FolderChangesDto, FolderEntryDto, FolderFileDto } from "../bindings/bindings";

const ROOT = "/work/repo";

const hoisted = vi.hoisted(() => ({
  asked: [] as string[],
  entries: {} as Record<string, { name: string; isDir: boolean }[]>,
  file: { truncated: false } as { text?: string; truncated: boolean; image?: { mime: string; base64: string } },
  /** The host's side of the watch: what it answers with, and the way to push a later list. */
  tell: null as null | ((changes: FolderChangesDto) => void),
  watching: { changed: [] as FolderChangedDto[], partial: false } as FolderChangesDto,
}));

vi.mock("./folder", () => ({
  folderWatch: async (projectId: number, root: string): Promise<FolderChangesDto> => {
    hoisted.asked.push(`watch:${projectId}:${root}`);
    return hoisted.watching;
  },
  folderUnwatch: async () => { hoisted.asked.push("unwatch"); },
  onFolderChanged: async (take: (changes: FolderChangesDto) => void) => {
    hoisted.tell = take;
    return () => { hoisted.tell = null; hoisted.asked.push("unlisten"); };
  },
  folderEntries: async (_projectId: number, root: string, path: string[]): Promise<FolderEntryDto[]> => {
    hoisted.asked.push(`entries:${root}:${path.join("/")}`);
    return hoisted.entries[path.join("/")] ?? [];
  },
  folderRead: async (_projectId: number, root: string, path: string[]): Promise<FolderFileDto> => {
    hoisted.asked.push(`read:${root}:${path.join("/")}`);
    return hoisted.file;
  },
  folderOpenFile: async (_projectId: number, root: string, path: string[]) => {
    hoisted.asked.push(`open:${root}:${path.join("/")}`);
  },
  folderRevealFile: async (_projectId: number, root: string, path: string[]) => {
    hoisted.asked.push(`reveal:${root}:${path.join("/")}`);
  },
}));

// The folder the project is bound to, answered without a store.
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: ROOT, exists: true }],
    live: [{ path: ROOT, exists: true }],
    answered: true,
  }),
}));

// What a reference resolves to. The store is not here, and what is under test is what the panel
// does with the answer rather than how the answer is found.
vi.mock("../core/reads", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/reads")>()),
  resolveRef: async () => ({ kind: "task", id: 12, title: "a task", live: true }),
}));

import { FilesPanel } from "./FilesPanel";
import type { Pointed } from "./pointed";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

type Props = Parameters<typeof FilesPanel>[0];

async function draw(props: Partial<Props> = {}) {
  await act(async () => {
    const projectId = "projectId" in props ? (props.projectId ?? null) : 1;
    root.render(createElement(FilesPanel, { ...props, projectId }));
  });
  await settle();
}

/** One thing an agent pointed at, with the parts a row is drawn from. */
function point(over: Partial<Pointed> & Pick<Pointed, "target">): Pointed {
  return { at: over.target, why: "", cwd: ROOT, read: false, ...over };
}

function click(el: Element | null | undefined) {
  return act(async () => {
    el?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

const button = (text: string) =>
  [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(text));

beforeEach(() => {
  hoisted.asked = [];
  hoisted.entries = {};
  hoisted.file = { truncated: false };
  hoisted.tell = null;
  hoisted.watching = {
    changed: [{ path: ["notes", "a.md"], modified: new Date().toISOString() }],
    partial: false,
  };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the file face", () => {
  it("watches the project's folder, not a pane's", async () => {
    await draw();
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    expect(container.textContent).toContain("a.md");
    // The folder the file is in is shown beside it: two files called the same thing are told apart
    // by where they are, and the row shows the name because that is what is read.
    expect(container.textContent).toContain("notes");
  });

  it("redraws the row when the host says the folder moved", async () => {
    await draw();
    expect(container.textContent).toContain("a.md");
    await act(async () => {
      hoisted.tell?.({
        changed: [{ path: ["src", "main.rs"], modified: new Date().toISOString() }],
        partial: false,
      });
      await new Promise((r) => setTimeout(r, 0));
    });
    // The list is the host's, whole: what it says now is what is drawn, with nothing merged in.
    expect(container.textContent).toContain("main.rs");
    expect(container.textContent).not.toContain("a.md");
  });

  it("says out loud when only part of the folder is watched", async () => {
    hoisted.watching = { changed: [], partial: true };
    await draw();
    // An unwatched half looks exactly like a half where nothing happened, so it is said rather
    // than left to be assumed (`AMB-T-3604`).
    expect(container.textContent).toContain(t("files.partial"));
  });

  it("takes its watch down when the face goes away", async () => {
    await draw();
    await act(async () => { root.unmount(); });
    expect(hoisted.asked).toContain("unwatch");
    expect(hoisted.asked).toContain("unlisten");
    // Re-created so afterEach's unmount has something to work on.
    container.remove();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  it("draws what the focused pane pointed at, and opens the file it names", async () => {
    hoisted.file = { text: "# pointed", truncated: false };
    const read: string[] = [];
    await draw({
      pointed: {
        points: [point({ target: "notes/a.md", why: "the one that broke" })],
        name: "トップの直し",
        ended: false,
        onRead: (at) => read.push(at),
      },
    });
    expect(container.textContent).toContain("the one that broke");
    // The heading carries whose pane it is: the row below it belongs to the project, this one does
    // not (`AMB-T-3603`).
    expect(container.textContent).toContain("トップの直し");

    await click(button("notes/a.md"));
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:notes/a.md`);
    // Opened is the only reading of "read" this side can honestly make.
    expect(read).toEqual(["notes/a.md"]);
  });

  it("does not draw a target it cannot open as something that opens", async () => {
    await draw({
      pointed: {
        points: [point({ target: "/etc/passwd", why: "outside the folder" })],
        name: null,
        ended: false,
        onRead: () => {},
      },
    });
    expect(container.textContent).toContain("/etc/passwd");
    expect(button("/etc/passwd")).toBeUndefined();
  });

  it("says nothing while the agent works, and says the count once it stops", async () => {
    const running = {
      points: [point({ target: "a.md" }), point({ target: "b.md" })],
      name: null,
      ended: false,
      onRead: () => {},
    };
    await draw({ pointed: running });
    // The count, and not a word more: an agent at work is not the moment to interrupt.
    expect(container.textContent).not.toContain(t("files.unopened").replace("{n}", "2"));

    await draw({ pointed: { ...running, ended: true } });
    expect(container.textContent).toContain(tf("files.unopened", { n: 2 }));
  });

  it("hands a file to the machine rather than trying to be one", async () => {
    await draw();
    const row = button("a.md")!;
    await act(async () => {
      row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 20 }));
    });
    // The face reads and does not edit: what it offers is the reader's own applications.
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:notes/a.md`);

    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.reveal")));
    expect(hoisted.asked).toContain(`reveal:${ROOT}:notes/a.md`);
  });

  it("closes the menu on the next thing the person does", async () => {
    await draw();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeDefined();
    // A menu that outlived the next click would sit over rows it is no longer about.
    await act(async () => {
      document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeUndefined();
  });

  it("says a project with no folder has none, rather than drawing empty rows", async () => {
    await draw({ projectId: null });
    expect(container.textContent).toBe(t("files.noFolder"));
    expect(hoisted.asked).toEqual([]);
  });

  it("leaves the tree folded, and opens it one level at a time", async () => {
    hoisted.entries[""] = [{ name: "src", isDir: true }, { name: "README.md", isDir: false }];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false }];
    await draw();
    // Folded: nothing has been read about the folder itself yet.
    expect(hoisted.asked.filter((one) => one.startsWith("entries:"))).toEqual([]);

    await click(button(t("files.tree")));
    await settle();
    expect(hoisted.asked).toContain(`entries:${ROOT}:`);
    expect(container.textContent).toContain("README.md");
    // A folder inside it is a name until it is opened — its children cost nothing until then.
    expect(hoisted.asked).not.toContain(`entries:${ROOT}:src`);

    await click(button("src"));
    await settle();
    expect(hoisted.asked).toContain(`entries:${ROOT}:src`);
    expect(container.textContent).toContain("main.rs");
  });

  it("draws a Markdown file as Markdown", async () => {
    hoisted.file = { text: "# A heading", truncated: false };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:notes/a.md`);
    expect(container.querySelector("h1")?.textContent).toBe("A heading");
  });

  it("draws text that is not Markdown as it was written", async () => {
    hoisted.entries[""] = [{ name: "run.sh", isDir: false }];
    hoisted.file = { text: "#!/bin/sh\necho hi", truncated: false };
    await draw();
    await click(button(t("files.tree")));
    await settle();
    await click(button("run.sh"));
    await settle();
    // Not a heading: the hash in a shell script is a comment, and a name is what decides that.
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector("pre")?.textContent).toBe("#!/bin/sh\necho hi");
  });

  it("says so when the file is not something a panel can show", async () => {
    hoisted.file = { truncated: false };
    await draw();
    await click(button("a.md"));
    await settle();
    // The name says Markdown and the bytes say otherwise; the bytes win (`crate::folder`).
    expect(container.textContent).toContain(t("files.notText"));
  });

  it("draws a picture out of the bytes the host carried", async () => {
    hoisted.file = { truncated: false, image: { mime: "image/png", base64: "AAAA" } };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(container.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AAAA");
  });

  it("says out loud when only the head of a long file is shown", async () => {
    hoisted.file = { text: "x".repeat(10), truncated: true };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(container.textContent).toContain(t("files.cut"));
  });

  it("leaves this face when a reference in a file is followed", async () => {
    const left: number[] = [];
    hoisted.file = { text: "see AMB-T-12", truncated: false };
    await draw({ onOpenLedger: () => left.push(1) });
    await click(button("a.md"));
    await settle();
    // A reference selects on the other face. Following one from here without leaving would land on
    // a pane the reader cannot see — a link that looks alive and is not (`AMB-D-747`).
    const ref = [...container.querySelectorAll("a")].find((a) => a.textContent === "AMB-T-12");
    expect(ref).toBeDefined();
    await click(ref);
    expect(left).toEqual([1]);
  });
});
