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
  /** Go, and leave them standing. */
  leave: string;
  /** Stay. */
  cancel: string;
};

/**
 * What a way out asks when the sessions behind it are still holding something.
 *
 * **It names what is about to be lost, at the moment it is about to be lost.** Two roads end every
 * session in the process at once and neither can be taken back: ending the app (`./AppShell`,
 * `crate::quit`), and starting it again to come back on a newer build, which ends them the same way
 * (`../components/UpdateBanner`). Either way, a reservation one of those sessions made stays
 * `in_progress` with nothing left that could say whose it was — the volatile area goes with the
 * process (`AMB-D-758`). Nothing afterwards can notice that happened, so the only place it can be
 * said is here.
 *
 * **It names them, and moves nothing** (`AMB-D-855`). The numbers come from the volatile area, and
 * what that area answers is the newest row it still has — after a move made outside a pane, an older
 * one. A name that is out of date is a line to read; a hand-back driven from the same answer was a
 * write to the ledger on a fact the world had passed, and it is gone. Where a reservation goes from
 * here is the ledger's own question, asked where reservations are (`task status <id> todo`).
 *
 * So there are two answers: go, or stay. Going leaves every reservation standing, which is the state
 * a person stepping away from a machine for the night wants anyway.
 *
 * The road is taken on the press and can still refuse; a refusal leaves the question standing with it
 * underneath, nothing ended.
 */
export function HoldingAsk({ holding, words, onLeave, onCancel }: {
  /** The tasks the sessions in question are holding, as the volatile area has them. Never empty — a
   *  way out that loses nothing is not asked this question at all. */
  holding: readonly number[];
  /** What this particular way out is called, in the reader's language. */
  words: AskWords;
  /** Take the road and leave the reservations standing. */
  onLeave: () => Promise<void>;
  onCancel: () => void;
}) {
  // Pressed once. The road that acts ends with this box gone, and a second press before that lands
  // would ask the same thing of the host twice.
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
            onClick={() => go(onLeave)}
          >
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

/** The words the way out of the app asks in (`./AppShell`). */
export function quitWords(): AskWords {
  return {
    title: t("quit.confirm"),
    held: t("quit.holding"),
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
    leave: t("restart.anyway"),
    cancel: t("restart.cancel"),
  };
}

/**
 * What the volatile area says one session is holding, asked at the moment a way out is pressed
 * (`commands.rs::session_work`). It is `heldByAll`'s half of the question — nothing outside this
 * module asks about one session, the two roads here ending every one of them at once.
 *
 * **Read at the press and not kept.** The answer is only worth having about the instant it is acted
 * on — a reservation made a second ago is exactly the one nobody would think to look for.
 *
 * **Silence is no reservations, and that is the honest answer.** A pane with nothing running in it,
 * a window not running under Tauri, a read that failed: what none of them can say is that something
 * is being left behind, and a question raised on a guess would be a question about nothing
 * (`AMB-D-758` — a move made outside a pane is not written here at all, and may not be guessed back).
 */
async function heldBy(session: string | null): Promise<readonly number[]> {
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
