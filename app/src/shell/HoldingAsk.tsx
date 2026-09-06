import { useState } from "react";
import { createPortal } from "react-dom";
import type { PtySessionDto, SessionWorkDto } from "../bindings/bindings";
import { errText, t } from "../core/i18n";
import { taskRef } from "../core/idref";
import { invoke } from "../core/ipc";

/** The words one way out asks in. Every road that raises this box says its own. */
export type AskWords = {
  /** What is about to happen, and what it costs. */
  title: string;
  /** The line above the tasks, introducing them. */
  held: string;
  /** Hand every one of them back, then go. */
  handBack: string;
  /** Go, and leave them standing. */
  leave: string;
  /** Stay. */
  cancel: string;
};

/**
 * What a way out asks when the sessions behind it are still holding something.
 *
 * **It names what is about to be lost, at the moment it is about to be lost.** Three roads end a
 * session and none of them can be taken back: pressing `✕` on a pane (`./TerminalPane`), ending the
 * app, which ends every pane at once (`./AppShell`, `crate::quit`), and starting it again to come
 * back on a newer build, which ends them the same way (`../components/UpdateBanner`). However it
 * goes, a reservation that session made stays `in_progress` with nothing left that could say whose
 * it was — the volatile area goes with the process (`AMB-D-758`). Nothing afterwards can notice that
 * happened, so the only place it can be said is here.
 *
 * **The three answers are three different things to want**, which is why this is a question and not a
 * confirmation: hand the work back and go, go and leave it standing, or stay. The middle one is not a
 * mistake to be talked out of — a person stepping away from a machine for the night has every reason
 * to leave a reservation where it is.
 *
 * **Nothing is moved until one of them is pressed.** The screen does not tidy the ledger up on its
 * own: a reservation is a fact somebody made, and the only thing that may unmake it is somebody.
 *
 * A hand-back that is refused leaves the question standing with the refusal under it. Nothing is
 * ended in that case — the way out was asked for *with* the work handed back, and doing half of it
 * would lose the very thing that was just named.
 */
export function HoldingAsk({ holding, words, onHandBack, onLeave, onCancel }: {
  /** The tasks the sessions in question are holding, as the volatile area has them. Never empty — a
   *  way out that loses nothing is not asked this question at all. */
  holding: readonly number[];
  /** What this particular way out is called, in the reader's language. */
  words: AskWords;
  /** Hand every one of them back to `todo`, then take the road. */
  onHandBack: () => Promise<void>;
  /** Take the road and leave the reservations standing. */
  onLeave: () => Promise<void>;
  onCancel: () => void;
}) {
  // Pressed once. Both roads that act end with this box gone, and a second press before that lands
  // would ask the same thing of the store twice.
  const [busy, setBusy] = useState(false);
  // A refusal from the road just taken, kept under the question rather than in place of it.
  const [failed, setFailed] = useState<string | null>(null);

  const go = (act: () => Promise<void>) => {
    setBusy(true);
    setFailed(null);
    void act().catch((e: unknown) => {
      setFailed(errText(e));
      setBusy(false);
    });
  };

  return createPortal(
    <div
      className="modal__overlay modal__overlay--raised"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onCancel(); }}
      onKeyDown={(e) => { if (e.key === "Escape") onCancel(); }}
    >
      <div className="holdingask__modal" role="dialog" aria-modal="true" aria-labelledby="holdingask-title">
        <div className="holdingask__title" id="holdingask-title">{words.title}</div>
        <div className="holdingask__holding">{words.held}</div>
        <ul className="holdingask__refs">
          {holding.map((id) => <li key={id}>{taskRef(id)}</li>)}
        </ul>
        {failed !== null && <p className="holdingask__failed">{failed}</p>}
        <div className="holdingask__actions">
          <button
            className="holdingask__action holdingask__action--go"
            autoFocus
            disabled={busy}
            onClick={() => go(onHandBack)}
          >
            {words.handBack}
          </button>
          <button className="holdingask__action" disabled={busy} onClick={() => go(onLeave)}>
            {words.leave}
          </button>
          <button className="holdingask__action" disabled={busy} onClick={onCancel}>
            {words.cancel}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

/** The words the way out of one pane asks in (`./TerminalPane`). */
export function paneDropWords(): AskWords {
  return {
    title: t("face.dropConfirm"),
    held: t("face.dropHolding"),
    handBack: t("face.dropHandBack"),
    leave: t("face.dropAnyway"),
    cancel: t("face.dropCancel"),
  };
}

/** The words the way out of the app asks in (`./AppShell`). */
export function quitWords(): AskWords {
  return {
    title: t("quit.confirm"),
    held: t("quit.holding"),
    handBack: t("quit.handBack"),
    leave: t("quit.anyway"),
    cancel: t("quit.cancel"),
  };
}

/**
 * The words the restart that applies an update asks in (`../components/UpdateBanner`).
 *
 * Starting again is not a smaller thing than ending: the process it comes back as is a new one, and
 * every session in the old one is gone the same way quitting loses them.
 *
 * The other restart — the one the overtaking gate offers (`../screens/RestartGate`) — raises no box
 * and so has no words of its own beyond `restart.confirm`. It cannot read what a session is holding,
 * because that answer comes through the store it is stuck on.
 */
export function restartWords(): AskWords {
  return {
    title: t("restart.confirm"),
    held: t("restart.holding"),
    handBack: t("restart.handBack"),
    leave: t("restart.anyway"),
    cancel: t("restart.cancel"),
  };
}

/**
 * What the volatile area says a session is holding, asked at the moment a way out is pressed
 * (`commands.rs::session_work`).
 *
 * **Read at the press and not kept.** The answer is only worth having about the instant it is acted
 * on — a reservation made a second ago is exactly the one nobody would think to look for.
 *
 * **Silence is no reservations, and that is the honest answer.** A pane with nothing running in it,
 * a window not running under Tauri, a read that failed: what none of them can say is that something
 * is being left behind, and a question raised on a guess would be a question about nothing
 * (`AMB-D-758` — a move made outside a pane is not written here at all, and may not be guessed back).
 */
export async function heldBy(session: string | null): Promise<readonly number[]> {
  if (session === null) return [];
  return invoke<SessionWorkDto>("session_work", { session })
    .then((work) => work.holding)
    .catch(() => []);
}

/**
 * The same question of every session this process has open, which is what the end of the app is
 * about to take with it (`crate::pty::pty_sessions`).
 *
 * One task can be reserved by only one session, so the answers do not overlap — but they are put
 * through a set anyway, because a list that named the same task twice would read as two things
 * being lost.
 */
export async function heldByAll(): Promise<readonly number[]> {
  const open = await invoke<PtySessionDto[]>("pty_sessions").catch(() => [] as PtySessionDto[]);
  const held = await Promise.all(open.map((one) => heldBy(one.session)));
  return [...new Set(held.flat())].sort((a, b) => a - b);
}

/**
 * How many panes this process has open (`crate::pty::pty_sessions`) — whether a road that ends all
 * of them at once has anything to end.
 *
 * It is asked separately from what they hold because the two answers fail apart. A store that cannot
 * be opened takes `heldByAll` down to an empty list while the terminals are still running and still
 * about to be lost, which is exactly the state the overtaking gate restarts out of
 * (`../screens/RestartGate`). Counting the panes needs no store, so that road can still ask.
 */
export async function openPanes(): Promise<number> {
  return invoke<PtySessionDto[]>("pty_sessions").then((open) => open.length).catch(() => 0);
}
