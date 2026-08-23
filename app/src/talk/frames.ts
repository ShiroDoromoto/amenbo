// What the panes of the talk window are called.
//
// **A name belongs to the frame, not to the session in it.** A frame is the place a terminal is drawn;
// the process in it comes and goes. Tied to the session, a name would come back on the next process
// started there — a pane still called "the migration" running something else entirely.
//
// So the name is kept, where nothing about a running session is (`./sessions`): it is in the store's
// device row, because frames are one machine's arrangement of one screen.
//
// **Three things name a frame and they are ranked** — the first line the person typed into it, then
// `session name` from the agent running in it, then the person saying so, which is the last word for
// good. The ranking itself is the store's (`amenbo_core::frames`), so this file never decides whether a
// name takes: it says who is naming and draws what comes back.

import type { FrameNameDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";

/** Who is naming a frame. Ranked in the store, lowest first. */
export type NamedBy = "typed" | "session" | "person";

/** The frame the split-out window's one pane is. That window is a single pane by construction
 *  (`AMB-D-753`), so it is always the first of the arrangement's places; the board's face has the
 *  rest of them (`./layout`), and keeping an arrangement across runs is `AMB-T-3607`'s. */
export const ONLY_FRAME = "1";

/** Frame → what it is called. */
export type FrameNames = ReadonlyMap<string, string>;

function named(rows: FrameNameDto[]): FrameNames {
  return new Map(rows.map((row) => [row.frame, row.name]));
}

/** What this device calls its frames. */
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
  /** True on the keystroke that sent it. */
  readonly sent: boolean;
  /** How far into an escape sequence the keys have got, so the rest of one is passed over rather than
   *  typed into the name. Nothing to read: it is where the accumulator is. */
  readonly esc: "" | "esc" | "csi";
};

export const NOTHING_TYPED: Typing = { line: "", sent: false, esc: "" };

/**
 * Follow the keys a person presses, far enough to know what their first line said.
 *
 * This is not an emulator and does not try to be one: what it needs is the first line a person sends
 * into a fresh pane, to call the pane something better than a number. So it takes plain characters,
 * honours the two ways of taking one back, and passes over the keys that are not characters — an arrow
 * key, a paste's own brackets, a Ctrl-C. A name is a line of text, and the keys that are not text are
 * not part of one.
 *
 * A line that came out wrong is a name a person can change, which is what lets this stay this small.
 */
export function typed(so_far: Typing, data: string): Typing {
  let line = so_far.sent ? "" : so_far.line;
  let esc = so_far.sent ? "" : so_far.esc;
  for (const key of data) {
    // Inside an escape sequence: `ESC [` and `ESC O` open one that runs to a byte in `@`–`~`, and
    // anything else after `ESC` is a sequence of two.
    if (esc === "esc") {
      esc = key === "[" || key === "O" ? "csi" : "";
      continue;
    }
    if (esc === "csi") {
      if (key >= "@" && key <= "~") esc = "";
      continue;
    }
    if (key === "\x1b") {
      esc = "esc";
      continue;
    }
    if (key === "\r" || key === "\n") return { line: line.trim(), sent: true, esc: "" };
    // Backspace, either of the two bytes a terminal sends for it.
    if (key === "\x7f" || key === "\b") {
      line = line.slice(0, -1);
      continue;
    }
    // What is left below a space is a key that is not a character — Ctrl-C, a tab. None of them belong
    // in a name, and none of them end the line either.
    if (key >= " ") line += key;
  }
  return { line, sent: false, esc };
}
