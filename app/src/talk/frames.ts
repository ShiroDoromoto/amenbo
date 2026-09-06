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
// **Two things name a frame and they are ranked** — `talk name` from the agent running in it, then
// the person saying so, which is the last word for good. The ranking itself is the store's
// (`amenbo_core::frames`), so this file never decides whether a name takes: it says who is naming and
// draws what comes back.
//
// **The terminal is not one of the two.** What a program in a pane sends — the title it asks for
// (OSC 0 / 2), the colour it answers with — is decided by whoever wrote it, and a name taken from
// there is a name amenbo cannot mean anything by (`AMB-D-748`); that is how `10;rgb:ecec/e9e9/…` came
// to be a pane's name (`AMB-T-3668`). **Neither is the person's own typing.** A first line was read
// off the key presses for a while (`AMB-T-4467`), and what it drew was not what had been sent: a
// press carries one key, so a slash command chosen from a menu left a pane called `/`, and a paste
// never reached it at all.
//
// A frame nobody has named is called after the folder it works in, which is a fact rather than a
// message from anyone.

import type { FrameNameDto, TalkLayoutDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import type { Frame, SavedLayout } from "./layout";

/** Who is naming a frame. Ranked in the store, lowest first. */
export type NamedBy = "session" | "person";

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
 * an agent's `talk name` does not take a person's name back off a frame. Drawing what was asked for
 * would show a name that is not the frame's.
 */
export async function nameFrame(frame: string, name: string, by: NamedBy): Promise<FrameNames> {
  return named(await invoke<FrameNameDto[]>("name_frame", { frame, name, by }));
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
 * name the frame takes the row. It is what a pane is called in the meantime, and better than a
 * number: a person knows their own folders (`crate::session`).
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

/**
 * What each of a project's panes is called on the screen, in the order they were opened.
 *
 * A pane nobody has named is called after the folder it works in (`frameLabel`), and one that has not
 * been opened in a folder either is called where it is — the page it is on and its place on it,
 * counted the way the pages are.
 *
 * **Two panes in one folder keep their places beside them.** A folder is not a name anybody chose, so
 * two panes working in the same one read the same — and a label a reader cannot tell from another is
 * no use for going to a pane. A name that repeats is left alone: two panes a person called the same
 * thing is a person's own business.
 *
 * It is here rather than in the rail because the rail is not the only place a pane is named: the row
 * above the pane carries the same label (`./nameplate`), and the two saying different things about
 * the same pane would be two panes as far as the reader is concerned.
 */
export function paneLabels(
  panes: readonly Frame[],
  names: FrameNames,
  count: number,
): Map<string, string> {
  const folders = panes.map((frame) => (names.has(frame.id) ? null : folderName(frame.folder)));
  const shared = new Set(folders.filter((one, at) => one !== null && folders.indexOf(one) !== at));
  return new Map(
    panes.map((frame, at) => {
      const place = `${Math.floor(at / count) + 1}.${(at % count) + 1}`;
      const folder = folders[at];
      return [
        frame.id,
        names.get(frame.id)
          ?? (folder === null ? place : shared.has(folder) ? `${folder} ${place}` : folder),
      ];
    }),
  );
}
