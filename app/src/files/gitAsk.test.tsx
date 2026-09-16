// @vitest-environment jsdom
// The question git is waiting on, put to the person in the window (`AMB-D-913`).
//
// What has to be right here is what crosses back to git. The sentence git wrote is drawn as git
// wrote it and never rewritten; an empty box answered is an empty password and a dialog closed is
// nobody answering, and git reads those two as different things; "keep it" is offered only where
// there is a whole credential to keep; and nothing is left waiting when the face goes away, because
// the call on the other side stands for as long as nobody answers.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GitAskDto } from "../bindings/bindings";
import { t, tf } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** Everyone listening for the host's word that git is waiting. */
  takers: [] as ((ask: GitAskDto) => void)[],
  /** Every answer that went back to the host, in order. */
  said: [] as { id: number; said: string | null; save: boolean }[],
}));

vi.mock("./folder", () => ({
  onGitAsked: async (take: (ask: GitAskDto) => void) => {
    hoisted.takers.push(take);
    return () => { hoisted.takers = hoisted.takers.filter((one) => one !== take); };
  },
  folderGitAskpassSaid: async (id: number, said: string | null, save: boolean) => {
    hoisted.said.push({ id, said, save });
  },
}));

import { GitAsk } from "./GitAsk";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** One question, filled in around whatever a test cares about. */
const asks = (about: Partial<GitAskDto>): GitAskDto => ({
  id: 1,
  doing: "git push",
  asked: "Password for 'https://alice@github.com': ",
  secret: true,
  savable: true,
  ...about,
});

async function draw() {
  await act(async () => {
    root.render(createElement(GitAsk));
  });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

/** The host saying git is waiting on this one. */
async function arrive(ask: GitAskDto) {
  await act(async () => {
    for (const take of [...hoisted.takers]) take(ask);
    await new Promise((r) => setTimeout(r, 0));
  });
}

const dialog = (): HTMLElement | null => document.querySelector(".gitask");
const said = (): HTMLInputElement => document.querySelector<HTMLInputElement>(".gitask__said")!;
const save = (): HTMLInputElement => document.querySelector<HTMLInputElement>(".gitask__save input")!;

const buttonOf = (words: string): HTMLButtonElement =>
  [...document.querySelectorAll<HTMLButtonElement>(".gitask__action")]
    .find((one) => one.textContent === words)!;

/** Press one thing, and let whatever it asked for come back. */
async function press(what: HTMLElement) {
  await act(async () => {
    what.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Type into the box, in place of whatever is in it. */
async function type(words: string) {
  const at = said();
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(at, words);
    at.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  hoisted.takers = [];
  hoisted.said = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the question git is waiting on", () => {
  it("is not drawn until git asks something", async () => {
    await draw();
    expect(dialog()).toBeNull();
  });

  it("draws git's own sentence as git wrote it, and says which call is waiting", async () => {
    await draw();
    await arrive(asks({ asked: "Password for 'https://alice@github.com': ", doing: "git push" }));
    expect(document.querySelector(".gitask__asked")?.textContent)
      .toBe("Password for 'https://alice@github.com': ");
    expect(document.querySelector(".gitask__doing")?.textContent)
      .toBe(tf("git.askDoing", { what: "git push" }));
  });

  it("hides what is typed, except where git is asking who the credential is for", async () => {
    await draw();
    await arrive(asks({ secret: false, asked: "Username for 'https://github.com': " }));
    expect(said().type).toBe("text");
    await press(buttonOf(t("git.askCancel")));
    await arrive(asks({ id: 2, secret: true }));
    expect(said().type).toBe("password");
  });

  it("sends what was typed back, and whether it is to be kept", async () => {
    await draw();
    await arrive(asks({ id: 7 }));
    await type("hunter2");
    await press(save());
    await press(buttonOf(t("git.askGo")));
    expect(hoisted.said).toEqual([{ id: 7, said: "hunter2", save: true }]);
    expect(dialog()).toBeNull();
  });

  /** git reads an empty password as a password, so the box answered empty is an answer. */
  it("answers an empty box as an empty answer, not as nobody answering", async () => {
    await draw();
    await arrive(asks({ id: 8 }));
    await press(buttonOf(t("git.askGo")));
    expect(hoisted.said).toEqual([{ id: 8, said: "", save: false }]);
  });

  /** And closing it is the other of the two: nobody was there. */
  it("answers a dialog closed with nothing at all", async () => {
    await draw();
    await arrive(asks({ id: 9 }));
    await type("hunter2");
    await press(buttonOf(t("git.askCancel")));
    expect(hoisted.said).toEqual([{ id: 9, said: null, save: false }]);
  });

  it("offers keeping it only where there is a whole credential to keep", async () => {
    await draw();
    await arrive(asks({ savable: false, asked: "Enter passphrase for key '/home/x/.ssh/id_ed25519': " }));
    expect(document.querySelector(".gitask__save")).toBeNull();
  });

  /** A `git push` over HTTPS asks twice, and the second is not the first being withdrawn. */
  it("holds the second question until the first is answered, with a box of its own", async () => {
    await draw();
    await arrive(asks({ id: 1, secret: false, asked: "Username for 'https://github.com': " }));
    await arrive(asks({ id: 2, secret: true, asked: "Password for 'https://alice@github.com': " }));
    expect(said().type).toBe("text");
    await type("alice");
    await press(buttonOf(t("git.askGo")));
    expect(said().type).toBe("password");
    expect(said().value).toBe("");
    await type("hunter2");
    await press(buttonOf(t("git.askGo")));
    expect(hoisted.said).toEqual([
      { id: 1, said: "alice", save: false },
      { id: 2, said: "hunter2", save: false },
    ]);
  });

  /** The call on the other side stands for as long as nobody answers — ten minutes, where the face
   *  went away without a word. */
  it("tells git nobody answered when the face goes away with a question still up", async () => {
    await draw();
    await arrive(asks({ id: 4 }));
    await act(() => root.unmount());
    expect(hoisted.said).toEqual([{ id: 4, said: null, save: false }]);
    // The unmount in `afterEach` must not answer it a second time.
    root = createRoot(container);
  });
});
