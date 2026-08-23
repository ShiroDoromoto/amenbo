// One pane of the talk window: a real terminal, drawn by xterm.js over a PTY the host holds.
//
// Nothing here interprets what crosses. A chunk of output arrives base64-encoded because the bytes
// are not text — an escape sequence is split wherever the host's read ended, and a multi-byte
// character with it — and it is handed to the emulator exactly as it came, which is the one thing
// that can put the split ones back together. Keystrokes go the other way just as plainly: what the
// emulator produced for the key is what the program in the terminal is given, so arrow keys, Ctrl-C,
// tab completion and bracketed paste all work because nothing tried to make them work.

import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { PtyChunkDto, SessionSaidDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { NOTHING_TYPED, typed, type NamedBy } from "./frames";

// The events the host sends this pane. Output is a chunk; closed is the program in the terminal
// having exited, which arrives once and is the last thing that session says.
const OUTPUT_EVENT = "pty://output";
const CLOSED_EVENT = "pty://closed";

// What the agent in this pane says about its session, as the host reads it out of the drop box it was
// given (`AMB-D-749`). It is the one thing crossing this seam that has been read rather than carried:
// the surface layer is a vocabulary, and a verb of it is exactly as good as its word.
const SAID_EVENT = "session://said";

/**
 * What a pane tells the window it is in. The window holds what is known about its sessions and what its
 * frames are called; a pane is where those things happen, not where they are kept.
 */
export type PaneEvents = {
  /** A terminal has started in this pane, under this session id. */
  opened(session: string, startedAt: string): void;
  /** The agent said something about its session. */
  said(statement: SessionSaidDto): void;
  /** The program in the terminal has exited. Nothing running is kept. */
  closed(session: string): void;
  /** Something has named this pane's frame. Whether the name takes is the store's to say — a person's
   *  name for a frame is not taken back off it by the agent (`./frames`). */
  name(name: string, by: NamedBy): void;
};

// The terminal's own colours, taken from the same tokens the rest of the interface is drawn from so
// a pane does not sit on the page as a black rectangle. Only the chrome follows the theme: the
// program inside cannot be told the colours changed (xterm.js does not implement the sequence that
// announces it), so what a TUI paints for itself stays as it chose.
function themeColors(): { background: string; foreground: string; cursor: string } {
  const style = getComputedStyle(document.documentElement);
  const token = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    background: token("--c-surface", "#ffffff"),
    foreground: token("--c-text", "#23211c"),
    cursor: token("--c-accent", "#0e7c7b"),
  };
}

// A chunk as it travelled: base64 in, bytes out. `atob` gives one character per byte, which is what
// makes the char codes the bytes themselves.
function decode(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Where a pane opens, and with what running in it (`./agent`). */
export type PaneStart = {
  /** The folder the shell starts in — canonical, as `wake_probe` answered with it. With none, the
   *  shell starts where a shell handed no directory lands, which is the person's home. */
  cwd?: string | null;
  /**
   * The catalogued id of the agent to start (`crate::wake`). A pane with none is a bare prompt.
   * What crosses is the id and never a command line: the catalog on the host side turns it into
   * one, so nothing here can name a program.
   */
  agent?: string | null;
};

/**
 * Fill `host` with a terminal, start a session in it, and return the way to take it away again.
 *
 * The host element is measured for the size in characters, and re-measured whenever it changes, so
 * what the program inside reads as the terminal's width is the pane's actual width — that is what a
 * full-screen interface reflows to.
 *
 * `on` is how the window is told what happened here — the session that started, what the agent said
 * about it, and the name the pane's frame should carry. `start` is where the terminal opens and what
 * is started in it, which the frame around the pane has already settled (`./agent`).
 */
export async function mountTerminal(
  host: HTMLElement,
  on: PaneEvents,
  start: PaneStart = {},
): Promise<() => void> {
  const term = new Terminal({
    fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim() ||
      "ui-monospace, monospace",
    fontSize: 13,
    cursorBlink: true,
    theme: themeColors(),
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(host);
  fit.fit();

  // The session is not known until the host answers, and the shell's first prompt can be on its way
  // before that answer lands. Listening first and holding what arrives is what keeps the prompt from
  // being the one thing the pane never shows.
  let session: string | null = null;
  const held: PtyChunkDto[] = [];
  let closed = false;

  const { listen } = await import("@tauri-apps/api/event");
  const unlistenOutput = await listen<PtyChunkDto>(OUTPUT_EVENT, ({ payload }) => {
    if (session === null) held.push(payload);
    else if (payload.session === session) term.write(decode(payload.base64));
  });
  const unlistenClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => {
    if (payload === session) {
      closed = true;
      on.closed(payload);
    }
  });
  // Statements are held the same way the output is, and for the same reason: the host starts watching
  // the drop box the moment it opens the terminal, so the first thing an agent says can be on its way
  // before the id it was said under is known here.
  const saidBeforeKnown: SessionSaidDto[] = [];
  const take = (statement: SessionSaidDto) => {
    on.said(statement);
    if (statement.verb === "name" && statement.text) on.name(statement.text, "session");
  };
  const unlistenSaid = await listen<SessionSaidDto>(SAID_EVENT, ({ payload }) => {
    if (session === null) saidBeforeKnown.push(payload);
    else if (payload.session === session) take(payload);
  });

  session = await invoke<string>("pty_open", {
    cwd: start.cwd ?? null,
    agent: start.agent ?? null,
    cols: term.cols,
    rows: term.rows,
  });
  on.opened(session, new Date().toISOString());
  for (const chunk of held.splice(0)) {
    if (chunk.session === session) term.write(decode(chunk.base64));
  }
  for (const statement of saidBeforeKnown.splice(0)) {
    if (statement.session === session) take(statement);
  }

  // The first line a person sends into this pane names its frame, so a pane is called something before
  // anyone gets round to naming it. Only the first: the keys are followed until one line has been sent
  // and then let alone, and whether the name takes at all is the store's to say (`./frames`).
  let typing = NOTHING_TYPED;
  const keys = term.onData((data) => {
    void invoke("pty_write", { session, data }).catch(() => {});
    if (typing.sent) return;
    typing = typed(typing, data);
    if (typing.sent && typing.line) on.name(typing.line, "typed");
  });

  // Re-measure on every change of the pane's size, and tell the host what the new size is. Both
  // halves are needed: the first reflows what is already drawn, the second is what wakes the program
  // inside so it repaints at the new width.
  const resize = new ResizeObserver(() => {
    fit.fit();
    void invoke("pty_resize", { session, cols: term.cols, rows: term.rows }).catch(() => {});
  });
  resize.observe(host);

  // The theme is settled on the document element, so a change of it is an attribute change there.
  const theme = new MutationObserver(() => {
    term.options.theme = themeColors();
  });
  theme.observe(document.documentElement, { attributeFilter: ["data-theme"] });

  return () => {
    resize.disconnect();
    theme.disconnect();
    keys.dispose();
    void unlistenOutput();
    void unlistenClosed();
    void unlistenSaid();
    if (!closed) void invoke("pty_close", { session }).catch(() => {});
    term.dispose();
  };
}
