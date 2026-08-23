// Which agent a pane opens with, and the row a frame offers once the program in it has ended.
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
// **The switch sits on the row of a frame that has closed, and only there.** A pane that is running
// holds a live process, and there is nothing a change of agent could mean for it short of killing
// what the person is in the middle of — so while it runs there is no control to press. What ending a
// program leaves is the frame with its last output still in it, and *that* is where the row appears:
// the agent to use, and open.
//
// Naming is not this module's (`./frames`): a frame is called what a person or the agent called it,
// and which agent it opens with is a different question about the same frame.
import type { WakeCandidateDto, WakeDto } from "../bindings/bindings";
import { errText, type Lang, t, tf } from "../core/i18n";
import { invoke } from "../core/ipc";
import { mountTerminal, type PaneEvents } from "./terminal";

/**
 * Fill `host` with the frame, and keep it filled for as long as the window is open. `on` is passed
 * straight through to whatever pane is running — this module decides what opens, not what is heard.
 */
export async function mountAgentFrame(host: HTMLElement, lang: Lang, on: PaneEvents): Promise<void> {
  const frame = document.createElement("div");
  frame.className = "talk__frame";
  host.append(frame);

  // The running pane's teardown, while there is one. A frame holds at most one terminal at a time,
  // and opening the next one is what takes the last one away — its final output stays on screen
  // until then, which is the whole reason a closed frame is still a frame.
  let close: (() => void) | null = null;
  let wake: WakeDto | null = null;
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

  /** Ask the host again, from nothing: the button on the notice, and the first draw. */
  const look = async () => {
    clear();
    frame.append(said("talk__looking", t("talk.searching", lang)));
    try {
      wake = await invoke<WakeDto>("wake_probe", { folder: null });
    } catch (e: unknown) {
      frame.replaceChildren(said("talk__failed", errText(e, lang)));
      return;
    }
    if (wake.settled) open(wake.settled);
    else if (wake.offered.length) ask(wake.offered);
    else nothing();
  };

  /** Open a pane on `agent`, replacing whatever the frame held. */
  const open = (agent: string) => {
    const folder = wake?.folder ?? null;
    clear();
    const mine = showing;
    const pane = document.createElement("div");
    pane.className = "talk__pane";
    frame.append(pane);
    const events: PaneEvents = {
      ...on,
      closed: (session) => {
        on.closed(session);
        if (mine === showing) frame.append(row(agent));
      },
    };
    void mountTerminal(pane, events, { cwd: folder, agent })
      .then((dispose) => {
        if (mine === showing) close = dispose;
        else dispose();
      })
      .catch((e: unknown) => {
        if (mine !== showing) return;
        pane.className = "talk__failed";
        pane.textContent = errText(e, lang);
        frame.append(row(agent));
      });
  };

  /** Keep this folder's answer, then open on it. A refusal to keep it is not a reason not to open. */
  const pick = (agent: string) => {
    void invoke("wake_remember", { folder: wake?.folder ?? null, agent }).catch(() => {});
    open(agent);
  };

  /** The offer, put once: several agents are startable here and only the person knows which. */
  const ask = (offered: string[]) => {
    clear();
    const box = document.createElement("div");
    box.className = "talk__ask";
    box.append(said("talk__askTitle", t("talk.ask", lang)));
    for (const one of named(wake, offered)) {
      const choose = document.createElement("button");
      choose.type = "button";
      choose.className = "talk__choice";
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
    box.className = "talk__ask";
    box.append(said("talk__askTitle", t("talk.none", lang)));
    box.append(
      said(
        "talk__hint",
        tf("talk.noneHint", { commands: (wake?.candidates ?? []).map((one) => one.command).join(", ") }, lang),
      ),
    );
    const again = document.createElement("button");
    again.type = "button";
    again.className = "talk__choice";
    again.textContent = t("talk.retry", lang);
    again.addEventListener("click", () => void look());
    box.append(again);
    frame.append(box);
  };

  /**
   * The closed frame's row: which agent the next pane opens with, and open.
   *
   * The list is only drawn where there is more than one to choose between — a row offering a choice
   * of one is a control that cannot do anything.
   */
  const row = (agent: string) => {
    const bar = document.createElement("div");
    bar.className = "talk__row";
    bar.append(said("talk__ended", t("talk.ended", lang)));

    const offered = named(wake, wake?.offered ?? []);
    let next = agent;
    if (offered.length > 1) {
      const label = document.createElement("label");
      label.className = "talk__pick";
      label.textContent = t("talk.agent", lang);
      const choose = document.createElement("select");
      for (const one of offered) {
        const option = document.createElement("option");
        option.value = one.id;
        option.textContent = one.label;
        option.selected = one.id === agent;
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
    again.className = "talk__choice";
    again.textContent = t("talk.open", lang);
    again.addEventListener("click", () => (next === agent ? open(next) : pick(next)));
    bar.append(again);
    return bar;
  };

  await look();
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
