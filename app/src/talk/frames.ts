// What the panes of the talk window are called.
//
// **A name belongs to the frame, not to the session in it.** A frame is the place a terminal is drawn;
// the process in it comes and goes. Tied to the session, a name would come back on the next process
// started there — a pane still called "the migration" running something else entirely.
//
// So the name is held where the frame is, and for as long: in the process, for this run only
// (`app/src-tauri/src/frames.rs`). Nothing about it is kept — ids start again at "1" on the next run,
// so a name kept against one would come back on a place nobody gave it to (`AMB-T-3687`). It is the
// host that holds it rather than the window, because the face moves between the two windows and a
// name belongs to the place wherever it is being drawn.
//
// **Three things name a frame and they are ranked** — the first line the person typed into it, then
// `session name` from the agent running in it, then the person saying so, which is the last word for
// good. The ranking itself is the store's (`amenbo_core::frames`), so this file never decides whether a
// name takes: it says who is naming and draws what comes back.
//
// **The terminal is not one of the three.** What a program in a pane sends — the title it asks for
// (OSC 0 / 2), the colour it answers with — is decided by whoever wrote it, and a name taken from
// there is a name amenbo cannot mean anything by (`AMB-D-748`). The first line is the *person's*
// keys, read off the presses and never off the stream leaving the pane: that stream carries the
// emulator's own answers to the program beside the person's typing, and nothing in it tells the two
// apart — which is how `10;rgb:ecec/e9e9/…` came to be a pane's name (`AMB-T-3668`).
//
// A frame nobody has named is called after the folder it works in, which is a fact rather than a
// message from anyone.

import type { FrameNameDto, TalkLayoutDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import type { SavedLayout } from "./layout";

/** Who is naming a frame. Ranked in the store, lowest first. */
export type NamedBy = "typed" | "session" | "person";

/** The first of the arrangement's places (`./layout`) — where a lone pane sits when nobody has said
 *  which of them it is drawing. */
export const ONLY_FRAME = "1";

/** Frame → what it is called. */
export type FrameNames = ReadonlyMap<string, string>;

function named(rows: FrameNameDto[]): FrameNames {
  return new Map(rows.map((row) => [row.frame, row.name]));
}

/** What this run calls its frames. */
export async function frameNames(): Promise<FrameNames> {
  return named(await invoke<FrameNameDto[]>("frame_names"));
}

/**
 * Name a frame, and answer with the names as they now stand.
 *
 * What comes back is the whole set rather than an acknowledgement, because a naming can be refused —
 * an agent's `session name` does not take a person's name back off a frame. Drawing what was asked for
 * would show a name that is not the frame's.
 */
export async function nameFrame(frame: string, name: string, by: NamedBy): Promise<FrameNames> {
  return named(await invoke<FrameNameDto[]>("name_frame", { frame, name, by }));
}

/** What a person has typed into a pane so far, and whether they have sent it. */
export type Typing = {
  /** The line as it stands, with the editing they have done to it applied. */
  readonly line: string;
  /** True on the press that sent it. */
  readonly sent: boolean;
};

export const NOTHING_TYPED: Typing = { line: "", sent: false };

/**
 * What a person did to the line they are writing.
 *
 * Three things happen to a line and nothing else is part of one: characters go in, one comes back
 * out, or the line goes. A press that is none of them — an arrow key, a Ctrl-C, a tab — is not a
 * smaller version of one of the three; it is the person doing something else entirely.
 */
export type Pressed =
  /** Characters entered: a key, or the whole of what an input method settled. */
  | { readonly kind: "text"; readonly text: string }
  /** One character taken back. */
  | { readonly kind: "erase" }
  /** The line has gone. */
  | { readonly kind: "send" };

/**
 * What a key press does to the line being written, or nothing where it is not part of one.
 *
 * **It is read off the press and never off what the press sends.** A pane's stream carries the
 * person's typing and the emulator's answers to the program in it, and nothing there tells them
 * apart (`AMB-D-748`); a press is a person by construction.
 *
 * Presses made while an input method is composing are its keys rather than the person's: what they
 * are writing is settled when the composition ends, and the whole of it arrives then (`./terminal`).
 */
export function pressedKey(e: KeyboardEvent): Pressed | null {
  if (e.isComposing) return null;
  // Shift-Enter is the pane writing a newline into the line the person is still on (`./terminal`),
  // so it is not the press that sends — and a line still being written has not been sent.
  if (e.key === "Enter") return e.shiftKey ? null : { kind: "send" };
  if (e.key === "Backspace") return { kind: "erase" };
  // A key held with a modifier is a command rather than a character, whatever character it is on.
  if (e.ctrlKey || e.altKey || e.metaKey) return null;
  // Named keys — `ArrowUp`, `Tab`, `Escape`, `Dead` — are longer than the one character a key that
  // *is* a character carries. Counted in characters and not in code units, so an emoji is one.
  return [...e.key].length === 1 ? { kind: "text", text: e.key } : null;
}

/**
 * Follow the keys a person presses, far enough to know what their first line said.
 *
 * This is not an emulator and does not try to be one: what it needs is the first line a person sends
 * into a fresh pane, to call the pane something better than a number.
 *
 * A line that came out wrong is a name a person can change, which is what lets this stay this small.
 */
export function typed(so_far: Typing, did: Pressed): Typing {
  // A line that has been sent is behind them: the next thing they type starts the next line, which
  // is more typing rather than another name (`amenbo_core::frames`).
  const line = so_far.sent ? "" : so_far.line;
  switch (did.kind) {
    case "text":
      return { line: line + did.text, sent: false };
    // Counted in characters, so taking back an emoji takes back the emoji.
    case "erase":
      return { line: [...line].slice(0, -1).join(""), sent: false };
    case "send":
      return { line: line.trim(), sent: true };
  }
}

/**
 * The folder's own name — the last part of its path, in either separator.
 *
 * Null for a path with no parts to it, which is a machine's root and not a folder anyone is working
 * in.
 */
export function folderName(path: string | null | undefined): string | null {
  if (!path) return null;
  const parts = path.split(/[\\/]/).filter((part) => part !== "");
  return parts[parts.length - 1] ?? null;
}

/**
 * What a frame is called on the screen: the name it carries, else the folder it works in.
 *
 * **The folder is not a name and does not become one** — nothing is written, and the first thing to
 * name the frame takes the row. It is what a pane is called in the meantime, which is better than a
 * number for the same reason the first line typed is: a person knows their own folders
 * (`crate::session`).
 *
 * Null is a frame with neither, which is a place nothing has been opened in yet — what to call one of
 * those is the business of whatever is drawing it.
 */
export function frameLabel(names: FrameNames, frame: string, folder: string | null): string | null {
  return names.get(frame) ?? folderName(folder);
}

/**
 * The arrangement the face is laid out from, or nothing where there is none to read.
 *
 * Read once, as the face comes up. What comes back is a shape and no sessions, so nothing about it
 * starts anything (`./layout`). In the first window of a run it holds the split and the project and
 * no frames at all — the places are this run's, and the last one's went with it (`AMB-T-3687`).
 */
export async function savedLayout(): Promise<SavedLayout | null> {
  return await invoke<TalkLayoutDto | null>("talk_layout", {});
}

/**
 * Write the arrangement down as it stands, for the other window to read.
 *
 * Written as the window is changed rather than as it closes: a window that is killed, or a machine
 * that loses power, is exactly the case a person wants the split they chose back after.
 */
export async function keepLayout(layout: SavedLayout): Promise<void> {
  await invoke<void>("save_talk_layout", { layout });
}
