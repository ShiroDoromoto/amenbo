// One pane of the terminal face: a real terminal, drawn by xterm.js over a PTY the host holds.
//
// The pane is a drawing of a session, not the session. A terminal belongs to the process, so a pane
// can be taken away and put up again — in the window the user split it out into, back in the board
// when they folded it, or in the interface a language change rebuilt around it — and the program
// inside it never learns that any of it happened (`AMB-D-753`). What a pane cannot carry across is
// the emulator's scrollback, so what it draws first is the tail the host kept (`crate::pty`).
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
import type { PtyChunkDto, PtySessionDto, SessionSaidDto } from "../bindings/bindings";
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
  /** A terminal is running in this pane, under this session id, since `startedAt`. It is said of a
   *  terminal this pane adopted as much as of one it started: what the window holds is what is
   *  running in it, and a session that moved windows is running in the one it moved to. */
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

/**
 * Measure the pane and tell the terminal its size in characters, unless the pane is not on screen.
 *
 * A hidden pane measures zero, and fitting to zero is what would make the program inside reflow to
 * nothing while nobody was looking. Switching to the other face hides this one and leaves the
 * terminal running (`AMB-D-753`), so "no size" here means "not being shown", and the size it had is
 * the size to keep until it is shown again.
 */
function refit(fit: FitAddon, host: HTMLElement): boolean {
  if (host.clientWidth === 0 || host.clientHeight === 0) return false;
  fit.fit();
  return true;
}

/**
 * Put a terminal in front of this pane: the one already running, if there is one, else a new one.
 *
 * A pane comes up for two reasons it cannot tell apart from the inside — the user asked for a
 * terminal, or the pane it already had moved. Asking the host what is open answers both, and is the
 * only thing that can: a webview that went away took its emulator with it and left nothing behind to
 * be asked.
 */
async function draw(term: Terminal): Promise<PtySessionDto> {
  const open = await invoke<PtySessionDto[]>("pty_sessions").catch(() => [] as PtySessionDto[]);
  // One open session is the only count that names one without guessing. There is a single terminal in
  // the app today; when there are several, which pane draws which is the slots' to say and not this
  // (`AMB-T-3609`).
  if (open.length === 1) {
    try {
      const replay = await invoke<string>("pty_attach", { session: open[0].session });
      if (replay) term.write(decode(replay));
      return open[0];
    } catch {
      // It ended between the two calls. Opening one is what the pane was there to do anyway.
    }
  }
  return await invoke<PtySessionDto>("pty_open", { cwd: null, cols: term.cols, rows: term.rows });
}

/**
 * Fill `host` with a terminal — the one already running, or a new one — and return the way to take
 * the pane away again.
 *
 * The host element is measured for the size in characters, and re-measured whenever it changes, so
 * what the program inside reads as the terminal's width is the pane's actual width — that is what a
 * full-screen interface reflows to.
 *
 * `on` is how the window is told what happened here — the session running in the pane, what the agent
 * said about it, and the name the pane's frame should carry.
 *
 * What comes back takes the pane away and **leaves the terminal running** for whatever draws it next.
 * There is no other way to end a pane, because there is no way yet to end a terminal: nothing in the
 * interface says "close this", and a pane going away is always the pane moving (`AMB-T-3632`).
 */
export async function mountTerminal(host: HTMLElement, on: PaneEvents): Promise<() => void> {
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
  refit(fit, host);

  // The session is not known until the host answers, and what the terminal has to say can be on its
  // way before that answer lands — the first prompt of a shell being started, or the next line of a
  // build on a session being adopted. Listening first and holding what arrives is what keeps either
  // from being the one thing the pane never shows.
  let session: string | null = null;
  const held: PtyChunkDto[] = [];

  const { listen } = await import("@tauri-apps/api/event");
  const unlistenOutput = await listen<PtyChunkDto>(OUTPUT_EVENT, ({ payload }) => {
    if (session === null) held.push(payload);
    else if (payload.session === session) term.write(decode(payload.base64));
  });
  const unlistenClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => {
    if (payload === session) on.closed(payload);
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

  const running = await draw(term);
  session = running.session;
  // The host's own answer for when it began, not the moment this pane went up: a session that moved
  // windows started when it started, and a pane that said otherwise would have the window telling the
  // reader the wrong thing about how long their work has been running.
  on.opened(running.session, running.startedAt);
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
  // Being shown again after the other face was up is such a change, which is what brings a hidden pane
  // back to the size of the window it is in.
  const resize = new ResizeObserver(() => {
    if (!refit(fit, host)) return;
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
    term.dispose();
  };
}
