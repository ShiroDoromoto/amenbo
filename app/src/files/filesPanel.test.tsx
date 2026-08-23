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
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

async function draw(props: { projectId?: number | null; onOpenLedger?: () => void } = {}) {
  await act(async () => {
    const projectId = "projectId" in props ? (props.projectId ?? null) : 1;
    root.render(createElement(FilesPanel, { projectId, onOpenLedger: props.onOpenLedger }));
  });
  await settle();
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
