// Which agent a pane's terminal starts as, and the row a frame offers once the program has ended.
//
// A pane is a terminal with a program in it, and the program is the agent the person works with in
// this folder. Working that out is the host's (`crate::wake`) — the folder's trace times what this
// machine can start — and what is left here is the three shapes that answer can take:
//
// | the host says | this draws |
// |---|---|---|
// | one agent | the pane, opened, with nothing asked |
// | several | the offer, once; the pick is kept and never asked again |
// | none startable | what was looked for, and a way to look again |
//
// **Before any of that there is the folder**, because a terminal has to be started somewhere and only
// the person knows where. Choosing one is the whole of the first run: it is the folder the AI is shown,
// it is what makes the folder a project's, and it is what opens the terminal — one press, three
// meanings, and no word the reader has to be taught first (`AMB-T-3606`). A frame with no folder puts
// the invitation and nothing else, and it is asked for once per page rather than once per pane: the
// second frame on a screen opens in the folder the first settled (`./layout`).
//
// **Nothing is asked about a terminal that is already running.** A pane adopts one rather than
// starting it (`./terminal`), and what is running was settled when it started — asking again would be
// asking about a decision that has already been carried out. So the question is put exactly where a
// terminal would be started, and nowhere else. That holds for the folder too: a session says where it
// runs, so taking one up answers the question rather than raising it.
//
// **The switch sits on the row of a frame that has closed, and only there.** A pane that is running
// holds a live process, and there is nothing a change of agent could mean for it short of killing
// what the person is in the middle of — so while it runs there is no control to press. What ending a
// program leaves is the frame with its last output still in it, and *that* is where the row appears.
//
// Naming is not this module's (`./frames`): a frame is called what a person or the agent called it,
// and which agent it opens with is a different question about the same frame.
import type { PtySessionDto, WakeCandidateDto, WakeDto } from "../bindings/bindings";
import { errText, type Lang, t, tf } from "../core/i18n";
import { invoke } from "../core/ipc";
import { chooseWorkFolder } from "../core/mutations";
import { mountTerminal, type PaneEvents, type PaneStart } from "./terminal";

/**
 * Fill `host` with the frame — the pane, and whatever has to be put to the reader before or after
 * one — and return the way to take it away again.
 *
 * `on` is passed straight through to whatever pane is running: this module decides what starts, not
 * what is heard. `paneClass` is the class the terminal's box is drawn with, because the two faces
 * that draw a pane style theirs differently and neither's box is this module's to name.
 *
 * `start` is which terminal this frame is for. A slot that already had one takes it up again and asks
 * nothing (`./layout`); a folder given here is where the question is put and where a started terminal
 * opens, which is what keeps one page's panes in one project.
 */
export async function mountAgentFrame(
  host: HTMLElement,
  lang: Lang,
  on: PaneEvents,
  paneClass: string,
  start: PaneStart = {},
): Promise<() => void> {
  const frame = document.createElement("div");
  frame.className = "agent__frame";
  host.append(frame);

  // The running pane's teardown, while there is one. A frame holds at most one terminal at a time,
  // and opening the next one is what takes the last one away — its final output stays on screen
  // until then, which is the whole reason a closed frame is still a frame.
  let close: (() => void) | null = null;
  let wake: WakeDto | null = null;
  // Where this frame's terminals are started. It begins as the page's — every pane on one screen opens
  // in one folder (`./layout`) — and is null only where nothing has been started on that page yet. That
  // is the frame with the invitation on it; once a folder is answered, by the person choosing one or by
  // a terminal this frame took up saying where it runs, it is not asked for again.
  let folder: string | null = start.cwd ?? null;
  // Which pane the frame is on. A terminal takes a round trip to mount, and the frame can be cleared
  // while one is in flight — so what comes back is checked against this and thrown away if the frame
  // has moved on. Without it a pane nobody can see keeps its PTY open for the life of the window.
  let showing = 0;

  const clear = () => {
    showing += 1;
    close?.();
    close = null;
    frame.replaceChildren();
  };

  /** Ask the host what this folder opens with — unless there is a terminal to adopt, which is an
   *  answer already carried out, or no folder has been chosen yet, which is a question that comes
   *  first. The buttons on the notice and the invitation both come back through here. */
  const look = async () => {
    clear();
    const running = await invoke<PtySessionDto[]>("pty_sessions").catch(() => [] as PtySessionDto[]);
    // This slot's own terminal, where it still has one. Otherwise a single open session, and only for
    // the slot that may take one up: with several panes on the screen, every one of them adopting the
    // one running terminal would leave the rest empty and the person short of the panes they asked for.
    const mine = start.session !== null && start.session !== undefined
      ? running.some((one) => one.session === start.session)
      : start.adopt !== false && running.length === 1;
    if (mine) return open(null, start);
    // Nothing to take up, and nowhere to start: the folder is what this frame is short of, and asking
    // for it is the whole of what it can do until it has one.
    if (folder === null) return invite(null);
    frame.append(said("agent__looking", t("talk.searching", lang)));
    try {
      wake = await invoke<WakeDto>("wake_probe", { folder });
    } catch (e: unknown) {
      frame.replaceChildren(said("agent__failed", errText(e, lang)));
      return;
    }
    if (wake.settled) open(wake.settled);
    else if (wake.offered.length) ask(wake.offered);
    else nothing();
  };

  /** Put up a pane, replacing whatever the frame held. `agent` is what a started terminal runs; a
   *  pane that adopts one is given none, and the running program is whatever it already was. */
  const open = (agent: string | null, take: PaneStart = { adopt: false }) => {
    // The host's spelling of the folder where there is one, because a probe canonicalises what it was
    // given and a terminal started under the other spelling is a terminal in a folder nothing else
    // names. A pane that takes one up was never probed, and starts nothing anyway.
    const cwd = wake?.folder ?? folder;
    clear();
    const mine = showing;
    const pane = document.createElement("div");
    pane.className = paneClass;
    frame.append(pane);
    const events: PaneEvents = {
      ...on,
      opened: (session, startedAt, where) => {
        // What is running here settles the frame's folder — which for a terminal this pane took up is
        // the answer the invitation would otherwise have asked for a second time.
        folder = where ?? folder;
        on.opened(session, startedAt, where);
      },
      closed: (session) => {
        on.closed(session);
        if (mine === showing) frame.append(row(agent));
      },
    };
    void mountTerminal(pane, events, { ...take, cwd, agent })
      .then((dispose) => {
        if (mine === showing) close = dispose;
        else dispose();
      })
      .catch((e: unknown) => {
        if (mine !== showing) return;
        pane.className = "agent__failed";
        pane.textContent = errText(e, lang);
        frame.append(row(agent));
      });
  };

  /** Keep this folder's answer, then open on it. A refusal to keep it is not a reason not to open. */
  const pick = (agent: string) => {
    void invoke("wake_remember", { folder: wake?.folder ?? folder, agent }).catch(() => {});
    open(agent);
  };

  /**
   * The first run: one line about what choosing a folder means, and the one button that does it.
   *
   * The line says what the folder *is* rather than what the AI will do with it — an agent's own
   * behaviour is not Amenbo's to promise (`AMB-D-749`), and a sentence about a confirmation that some
   * tools do not ask for would be a lie on the first screen a reader ever sees. What is true of every
   * one of them is where they can look.
   *
   * `note` is a refusal from the last attempt, kept under the invitation rather than in place of it:
   * a folder that could not be bound leaves the reader exactly where they were, which is with a folder
   * still to choose.
   */
  const invite = (note: string | null) => {
    clear();
    const box = document.createElement("div");
    box.className = "agent__ask";
    box.append(said("agent__askTitle", t("talk.folder", lang)));
    const choose = document.createElement("button");
    choose.type = "button";
    choose.className = "agent__choice";
    choose.textContent = t("talk.chooseFolder", lang);
    choose.addEventListener("click", () => {
      choose.disabled = true;
      void chooseWorkFolder()
        .then((chosen) => {
          // Cancelling is not a refusal and not an answer: the invitation stands, ready to be pressed
          // again.
          if (chosen === null) {
            choose.disabled = false;
            return;
          }
          folder = chosen;
          return look();
        })
        .catch((e: unknown) => invite(errText(e, lang)));
    });
    box.append(choose);
    if (note !== null) box.append(said("agent__failed", note));
    frame.append(box);
  };

  /** The offer, put once: several agents are startable here and only the person knows which. */
  const ask = (offered: string[]) => {
    clear();
    const box = document.createElement("div");
    box.className = "agent__ask";
    box.append(said("agent__askTitle", t("talk.ask", lang)));
    for (const one of named(wake, offered)) {
      const choose = document.createElement("button");
      choose.type = "button";
      choose.className = "agent__choice";
      choose.textContent = one.label;
      choose.addEventListener("click", () => pick(one.id));
      box.append(choose);
    }
    frame.append(box);
  };

  /** Nothing on this machine can be started: what was looked for, and the way to look again. */
  const nothing = () => {
    clear();
    const box = document.createElement("div");
    box.className = "agent__ask";
    box.append(said("agent__askTitle", t("talk.none", lang)));
    box.append(
      said(
        "agent__hint",
        tf("talk.noneHint", { commands: (wake?.candidates ?? []).map((one) => one.command).join(", ") }, lang),
      ),
    );
    const again = document.createElement("button");
    again.type = "button";
    again.className = "agent__choice";
    again.textContent = t("talk.retry", lang);
    again.addEventListener("click", () => void look());
    box.append(again);
    frame.append(box);
  };

  /**
   * The closed frame's row: which agent the next pane starts as, and open.
   *
   * That the program has ended is not said here — both faces already say it (`face.ended`), and the
   * same fact twice on one screen reads as two things having happened. The list is drawn only where
   * there is more than one to choose between: a row offering a choice of one is a control that cannot
   * do anything.
   */
  const row = (agent: string | null) => {
    const bar = document.createElement("div");
    bar.className = "agent__row";

    const offered = named(wake, wake?.offered ?? []);
    let next = agent ?? offered[0]?.id ?? null;
    if (offered.length > 1) {
      const label = document.createElement("label");
      label.className = "agent__pick";
      label.textContent = t("talk.agent", lang);
      const choose = document.createElement("select");
      for (const one of offered) {
        const option = document.createElement("option");
        option.value = one.id;
        option.textContent = one.label;
        option.selected = one.id === next;
        choose.append(option);
      }
      choose.addEventListener("change", () => {
        next = choose.value;
      });
      label.append(choose);
      bar.append(label);
    }

    const again = document.createElement("button");
    again.type = "button";
    again.className = "agent__choice";
    again.textContent = t("talk.open", lang);
    // A pane that adopted a terminal never asked, so there is nothing settled to reopen with: the
    // question is put now, which is the first moment there is a terminal to start rather than adopt.
    again.addEventListener("click", () => {
      if (next === null) void look();
      else if (next === agent) open(next);
      else pick(next);
    });
    bar.append(again);
    return bar;
  };

  await look();
  return clear;
}

/** The catalogue rows behind a list of ids, in the order the ids came in. */
function named(wake: WakeDto | null, ids: string[]): WakeCandidateDto[] {
  return ids.flatMap((id) => (wake?.candidates ?? []).filter((one) => one.id === id));
}

/** A line of the frame's own prose. */
function said(className: string, text: string): HTMLElement {
  const line = document.createElement("p");
  line.className = className;
  line.textContent = text;
  return line;
}
