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
// **Beside all three there is the shell** ({@link SHELL}) — the folder's own prompt with nothing
// started at it. It is put wherever this module puts a choice and never where it does not, so a
// folder that settled on one agent still opens on it unasked. What it is for is the terminal a
// reader wants after the agent has gone: `git status`, a build, a machine with no agent on it at all.
//
// **Before any of that there is the folder**, because a terminal has to be started somewhere and only
// the person knows where. Choosing one is the whole of the first run: it is the folder the AI is shown,
// it is what makes the folder a project's, and it is what opens the terminal — one press, three
// meanings, and no word the reader has to be taught first (`AMB-T-3606`). On the board's face that
// press happens **before** the pane is made, among the folders the project is bound to
// (`../shell/FolderChoice`), so a frame there always arrives with one. A frame with no folder puts the
// invitation and nothing else, which is an empty place on a page: room for a pane nobody has opened.
//
// **The invitation keeps to the same folders the board's does.** A pane belongs to a project, so what
// it may work in is what that project is bound to and nothing else — a frame that could reach any
// folder on the machine would put a pane from another project on a screen the rail promises cannot
// hold one. Which project that is comes from the face the frame sits on (`../shell/TerminalFace`),
// and the one case with no list to keep to is the machine with no project yet, where the press is the
// first run's and the folder chosen raises the project it belongs to.
//
// **What the empty frame chose is carried in rather than asked for again.** The face puts the
// startable agents on the frame itself and opens on one press ({@link PaneStart.agent} ·
// `../shell/AdriftSlot`), so a pane arriving with a choice has already been through the question
// this module would put — and putting it a second time would be asking about a decision the person
// has just made. What such a pane still needs from here is the probe: the folder's canonical form,
// and whether the project had settled on anything, which is what says if this choice is the answer
// or one turn's departure from it.
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
import type { BoundFolderDto, PtySessionDto, WakeCandidateDto, WakeDto } from "../bindings/bindings";
import { errText, type Lang, t, tf } from "../core/i18n";
import { invoke } from "../core/ipc";
import { chooseFolderFor, chooseWorkFolder, fetchBoundFolders } from "../core/mutations";
import { mountTerminal, SHELL, type PaneEvents, type PaneStart } from "./terminal";

/** The class the terminal's box is drawn with. xterm.js measures the element it was opened in, so
 *  this is the rule that decides how many columns and rows the program inside is told it has
 *  (`app/src/styles/global.css`). One name, because there is one face that draws a pane — the board
 *  shows it and the split-out window shows the same one (`../shell/TerminalFace`). */
const PANE_CLASS = "termface__pane";

/**
 * Fill `host` with the frame — the pane, and whatever has to be put to the reader before or after
 * one — and return the way to take it away again.
 *
 * `on` is passed straight through to whatever pane is running: this module decides what starts, not
 * what is heard.
 *
 * `start` is which terminal this frame is for. A slot that already had one takes it up again and asks
 * nothing (`./layout`); a folder given here is where a started terminal opens. The face settles where
 * a pane works before the pane is made (`../shell/FolderChoice`), so the invitation below is for the
 * frame that arrived without one — a pane that took up a terminal somebody else started.
 *
 * `project` is which project this frame's pane is one of. It is what the invitation asks among —
 * that project's bound folders and nothing else — and it is **whose answer the agent is kept
 * against**: which agent a person works with is a thing about the work rather than about a
 * directory, and one project can bind several folders (`crate::wake`). Both faces pass it; null is a
 * machine that has no project yet, where the first folder chosen is what raises one, and a pane
 * there settles nothing until it does.
 */
export async function mountAgentFrame(
  host: HTMLElement,
  lang: Lang,
  on: PaneEvents,
  start: PaneStart = {},
  project: number | null = null,
): Promise<() => void> {
  const frame = document.createElement("div");
  frame.className = "agent__frame";
  host.append(frame);

  // The running pane's teardown, while there is one. A frame holds at most one terminal at a time,
  // and opening the next one is what takes the last one away — its final output stays on screen
  // until then, which is the whole reason a closed frame is still a frame.
  let close: (() => void) | null = null;
  let wake: WakeDto | null = null;
  // Where this frame's terminals are started. It is the folder the face settled for this pane
  // (`./layout`), and is null only for a frame nobody has settled one for — the pane that took up a
  // terminal somebody else started. That is the frame with the invitation on it; once a folder is
  // answered, by the person choosing one or by a terminal this frame took up saying where it runs,
  // it is not asked for again.
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
      wake = await invoke<WakeDto>("wake_probe", { folder, project });
    } catch (e: unknown) {
      frame.replaceChildren(said("agent__failed", errText(e, lang)));
      return;
    }
    // The empty frame's choice, where the pane arrived with one — settling the project's answer only
    // if it had none, the same as every other way one is chosen ({@link pick}).
    const chose = start.agent;
    if (chose !== null && chose !== undefined) return pick(chose);
    if (wake.settled) open(wake.settled);
    else if (wake.offered.length) ask(wake.offered);
    else nothing();
  };

  /** Put up a pane, replacing whatever the frame held. `choice` is what a started terminal runs — a
   *  catalogued agent, or {@link SHELL} for a prompt with nothing started at it; a pane that adopts
   *  one is given neither, and the running program is whatever it already was. */
  const open = (choice: string | null, take: PaneStart = { adopt: false }) => {
    // The host's spelling of the folder where there is one, because a probe canonicalises what it was
    // given and a terminal started under the other spelling is a terminal in a folder nothing else
    // names. A pane that takes one up was never probed, and starts nothing anyway.
    const cwd = wake?.folder ?? folder;
    // A shell is the absence of an agent rather than a program of its own, which is what the host is
    // told: the same "none" a pane that took a terminal up is given.
    const agent = choice === SHELL ? null : choice;
    clear();
    const mine = showing;
    const pane = document.createElement("div");
    pane.className = PANE_CLASS;
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
        if (mine === showing) frame.append(row(choice));
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
        frame.append(row(choice));
      });
  };

  /** The way to a prompt with nothing started at it, drawn wherever the frame is putting a choice
   *  ({@link SHELL}). It opens rather than remembering: a shell is not this folder's answer. */
  const shellChoice = () => {
    const choose = document.createElement("button");
    choose.type = "button";
    choose.className = "agent__choice";
    choose.textContent = t("talk.shell", lang);
    choose.addEventListener("click", () => open(SHELL));
    return choose;
  };

  /**
   * Open on this agent, and keep it as the project's answer where there was not one.
   *
   * **A project settles its agent once, and pressing another one does not re-settle it.** What a
   * person reaches for on one pane is that pane — the answer is theirs to change on the project's
   * own settings, and a face that rewrote it every time would turn one turn's departure into a
   * decision nobody made (`AMB-T-3667`).
   *
   * The shell is never kept either ({@link SHELL}): "which agent do you work with here" is not a
   * question a prompt with nothing started at it answers. A frame with no project has nowhere to
   * keep one, and opens all the same. A refusal to keep it is not a reason not to open.
   */
  const pick = (agent: string) => {
    if (agent !== SHELL && project !== null && !wake?.settled) {
      void invoke("wake_remember", { project, agent }).catch(() => {});
    }
    open(agent);
  };

  /** This frame has settled where it works. The window is told now rather than when a terminal
   *  starts: the two can be a long way apart, a machine with nothing startable never getting there
   *  at all, and what the window does with a folder is not this frame's to wait on. */
  const settle = async (chosen: string) => {
    folder = chosen;
    on.chose(chosen);
    await look();
  };

  /**
   * Where this pane works, asked of the person — and asked among this project's folders only.
   *
   * **A project bound to one folder is not a question**: the pane opens there and nothing is put on
   * the screen, which is the rule the board's face keeps too (`../shell/FolderChoice`). Bound to
   * several, the answers are those folders. Bound to none — and that is also every machine with no
   * project yet — there is no list to choose from, so the press is the first run's one press: it
   * says what the folder *is* rather than what the AI will do with it, because an agent's own
   * behaviour is not Amenbo's to promise (`AMB-D-749`) and a sentence about a confirmation some
   * tools do not ask for would be a lie on the first screen a reader ever sees.
   *
   * That press binds where it lands: to **this** project where the window has one, and to the
   * project the folder's own name raises where it has not (`../core/mutations`). A folder bound to
   * some other project would put this pane outside the project it belongs to, which is the one thing
   * the rail promises cannot happen.
   *
   * `note` is a refusal from the last attempt, kept under the invitation rather than in place of it:
   * a folder that could not be bound leaves the reader exactly where they were, which is with a
   * folder still to choose.
   */
  const invite = async (note: string | null) => {
    // Read before the frame is emptied: what is on the screen is the invitation the reader is
    // looking at, and blanking it for the length of a store read would be a flicker for nothing.
    const bound = project === null ? [] : await folders(project);
    if (bound.length === 1) return await settle(bound[0]!.path);
    clear();
    const box = document.createElement("div");
    box.className = "agent__ask";
    box.append(said("agent__askTitle", t(bound.length === 0 ? "talk.folder" : "face.whichFolder", lang)));
    // Pressed once, whichever way in it is. Both roads end in a pane, and a second press before that
    // lands would open a second one.
    const press = (act: () => void) => {
      for (const one of box.querySelectorAll("button")) one.disabled = true;
      act();
    };
    if (bound.length === 0) {
      const choose = document.createElement("button");
      choose.type = "button";
      choose.className = "agent__choice";
      choose.textContent = t("talk.chooseFolder", lang);
      choose.addEventListener("click", () => press(() => {
        void (project === null ? chooseWorkFolder() : chooseFolderFor(project))
          .then((chosen) => {
            // Cancelling is not a refusal and not an answer: the invitation stands, ready to be
            // pressed again.
            if (chosen === null) {
              choose.disabled = false;
              return;
            }
            return settle(chosen);
          })
          .catch((e: unknown) => invite(errText(e, lang)));
      }));
      box.append(choose);
    } else {
      for (const one of bound) {
        const choose = document.createElement("button");
        choose.type = "button";
        choose.className = "agent__choice";
        choose.textContent = one.path;
        choose.addEventListener("click", () => press(() => void settle(one.path)));
        box.append(choose);
      }
    }
    if (note !== null) box.append(said("agent__failed", note));
    frame.append(box);
  };

  /** The offer, put once: several agents are startable here and only the person knows which — and
   *  under them the shell, for the reader who wants the folder's prompt and no agent at all. */
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
    box.append(shellChoice());
    frame.append(box);
  };

  /** No agent on this machine can be started: what was looked for, the way to look again, and the
   *  shell — which is the only terminal this machine has, and used to be what the face opened with. */
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
    box.append(shellChoice());
    frame.append(box);
  };

  /**
   * The closed frame's row: what the next pane starts as, and open.
   *
   * That the program has ended is not said here — both faces already say it (`face.ended`), and the
   * same fact twice on one screen reads as two things having happened. The list is drawn wherever
   * this folder has an agent to start, because the shell stands on it beside them ({@link SHELL}) and
   * there is always the other one to choose. A frame that never probed — a pane that took a terminal
   * up and then ended — has nothing to list, and its `open` puts the question instead.
   */
  const row = (choice: string | null) => {
    const bar = document.createElement("div");
    bar.className = "agent__row";

    const offered = named(wake, wake?.offered ?? []);
    let next = choice ?? offered[0]?.id ?? null;
    if (offered.length) {
      const label = document.createElement("label");
      label.className = "agent__pick";
      label.textContent = t("talk.startWith", lang);
      const choose = document.createElement("select");
      for (const one of [
        ...offered.map((agent) => ({ id: agent.id, label: agent.label })),
        { id: SHELL, label: t("talk.shell", lang) },
      ]) {
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
      else pick(next);
    });
    bar.append(again);
    return bar;
  };

  await look();
  return clear;
}

/**
 * The folders a project is bound to that are actually there — the only ones a terminal can be started
 * in, and the only ones a pane of that project may work in (`../shell/FolderChoice`).
 *
 * A read that would not come back is a project with nothing to offer, which leaves the press that
 * binds a folder to it: the invitation to link one is the safe half of being wrong, the way it is
 * wherever else this is asked (`../core/boundFolders`).
 */
async function folders(project: number): Promise<BoundFolderDto[]> {
  const bound = await fetchBoundFolders(project).catch(() => [] as BoundFolderDto[]);
  return bound.filter((one) => one.exists);
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
