// The question about having a folder's AI read amenbo's instruction at the start of every session
// (`AMB-D-440`), which on this surface is a modal. It is the GUI's half of the CLI's `offer_agent_hook`.
//
// **It is put where it is about.** What raises it is the project the reader has just opened, not a sweep of
// every bound folder at startup (`AMB-D-459`): the answer changes with the place, so the place is where it
// is read. It is still asked about a **state** — a project nothing is wired in — and never on the way to
// creating something, which is what reaches the projects that already exist rather than only the next ones.
//
// **The answer is per project**, unlike the lint's one-per-device consent, so the modal names the project
// and hands the answer back with it. One question covers the project's folders however many there are: the
// text is the same in each, and only the path it goes in differs.
//
// **A yes wires nothing, so the yes has to hand over the text.** amenbo does not write into a user's
// provider settings — it hands over a text for an AI of theirs to act on — so the whole of what a yes buys
// is the text, and this modal is where it arrives: the picker for which tool it is for, the text itself, and
// the copy button. The startup banner used to be that place; the question now finishes what it started
// (`AMB-D-459`).
//
// Three values, two buttons, exactly as `HookConsentModal` has them: dismissing is "not now" and records
// nothing, which is why Esc must not be wired to `answer(false)` — that answer means "never ask again".
import { useEffect, useRef, useState } from "react";
import { answerAgentHookOffer, fetchAgentHookOffer } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { AgentHookOfferDto } from "../bindings/bindings";

/**
 * `projectId` is the project on screen — null on any other view, where nothing is asked. `turn` is false
 * while the lint's modal is still asking, and nothing here is fetched or shown until it goes true. `canAsk`
 * is the rest of that rule — false when the lint actually put its question this run, in which case the
 * probe still runs (a project wired by hand is adopted without anyone being asked) but no question is
 * raised, and this one comes round the next time the project is opened.
 *
 * `onDone` fires once there is nothing left on screen — answered and read, waved past, or never there —
 * which is what lets the setup banner wait its turn. A failure to fetch counts as done, so a surface that
 * could not ask does not also mute the one that only tells. It can fire more than once, so it must be
 * idempotent.
 */
export function AgentHookConsentModal({ projectId, turn, canAsk, onDone }: {
  projectId: number | null;
  turn: boolean;
  canAsk: boolean;
  onDone?: () => void;
}) {
  const [offer, setOffer] = useState<AgentHookOfferDto | null>(null);
  // The same offer once it has been said yes to: the question is over, and what is left is the hand-over.
  const [handover, setHandover] = useState<AgentHookOfferDto | null>(null);
  const [asked, setAsked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Which tool the reader picked. Unset means the first on offer — the only one where the folders point at
  // exactly one tool, and the head of the catalog where they point at none.
  const [picked, setPicked] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // The projects whose question was put off. Dismissing records nothing on purpose, and the trigger is a
  // navigation — so without this, walking back into the project would put the same question again, which is
  // the nagging "not now" exists to avoid. It lives as long as the app is open, exactly as the dismissal
  // does: the next launch asks again, because nothing was answered.
  const putOff = useRef(new Set<number>());

  useEffect(() => {
    // Whatever was on screen belonged to the project we were on; a different one is a different question.
    setOffer(null);
    setHandover(null);
    setError(null);
    setPicked(null);
    setCopied(false);
    if (!turn) return; // The queue has not reached us; the banner is waiting on the lint's modal, not ours.
    if (projectId === null || putOff.current.has(projectId)) {
      setAsked(true); // Nothing to ask here, which the banner behind us is waiting to hear.
      return;
    }
    let alive = true;
    fetchAgentHookOffer(projectId, canAsk)
      .then((o) => alive && setOffer(o))
      .catch(() => {}) // A failure to detect is swallowed: we ask nothing rather than block the app.
      .finally(() => alive && setAsked(true));
    return () => {
      alive = false;
    };
    // `canAsk` is settled by the time `turn` goes true and never moves after, so the probe runs once per
    // project opened.
  }, [turn, canAsk, projectId]);

  useEffect(() => {
    if (asked && !offer && !handover) onDone?.();
  }, [asked, offer, handover, onDone]);

  // Dropping the question is the whole of "not now": nothing is called, so nothing is recorded, and a later
  // launch finds it waiting. Closing the hand-over is the same gesture with nothing left to decide.
  const dismiss = () => {
    setError(null);
    if (offer) putOff.current.add(offer.projectId);
    setOffer(null);
    setHandover(null);
  };

  // Esc is "not now", and deliberately not wired to a no — dismissing a dialog is how people put a question
  // off, and recording that as a refusal would answer on their behalf. Above the early return because a hook
  // cannot be reached conditionally; harmless with nothing up.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const answer = async (yes: boolean) => {
    if (!offer) return;
    setBusy(true);
    try {
      await answerAgentHookOffer(offer.projectId, yes);
      // A yes is the start of the hand-over, not the end of the exchange: what was agreed to is the text,
      // so it goes up in the same modal. A no is silence from here on, and closes it.
      if (yes) setHandover(offer);
      setOffer(null);
    } catch (e) {
      setError(errText(e)); // Nothing was recorded: leave the question up.
    } finally {
      setBusy(false);
    }
  };

  const copy = async (request: string) => {
    try {
      await navigator.clipboard.writeText(request);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };

  // Every folder the text goes in, each named with the project it belongs to. The text is one; the paths
  // are what differ, so they are listed rather than the question being asked once per folder (`AMB-D-459`).
  const folders = (which: AgentHookOfferDto) => (
    <div className="hookconsent__folders">
      {which.dirs.map((dir) => (
        <div key={dir}>{tf("agentHook.where", { project: which.projectName, dir })}</div>
      ))}
    </div>
  );

  if (handover) {
    const tool = handover.offered.find((one) => one.tool === picked) ?? handover.offered[0];
    return (
      <div className="hookconsent__overlay">
        <div className="hookconsent__modal" role="dialog" aria-modal="true" aria-labelledby="agenthook-title">
          <div className="hookconsent__title" id="agenthook-title">{t("agentHookSetup.title")}</div>
          {folders(handover)}
          {/* Only where there is a choice to make. With one on offer the folders have already said which
              tool it is, and a picker holding a single value asks a question that has no other answer. */}
          {handover.offered.length > 1 && (
            <select
              className="hookconsent__pick"
              aria-label={t("agentHookSetup.pick")}
              value={tool.tool}
              onChange={(e) => setPicked(e.target.value)}
            >
              {handover.offered.map((one) => (
                <option key={one.tool} value={one.tool}>{one.label}</option>
              ))}
            </select>
          )}
          <div className="hookconsent__why">{tf("agentHookSetup.unwired", { tool: tool.label, file: tool.pasteInto })}</div>
          {/* The text is on screen, not behind the button: what it asks for is an edit to a file the reader
              owns, by an AI of theirs, so the moment to read it is before it is handed over. */}
          <pre className="hookconsent__request">{tool.request}</pre>
          <div className="hookconsent__actions">
            <button className="hookconsent__action hookconsent__action--yes" onClick={() => void copy(tool.request)}>
              {copied ? t("agentHookSetup.copied") : t("agentHookSetup.copy")}
            </button>
            <button className="hookconsent__action" onClick={dismiss}>{t("pane.close")}</button>
          </div>
          <div className="hookconsent__hint">{tf("agentHook.hint", { cmd: handover.cmd })}</div>
        </div>
      </div>
    );
  }

  if (!offer) return null;

  // Named only where the folders point at exactly one tool. With several, which one is the reader's to say
  // — the picker after the yes is where they say it — and with none there is nothing to name. Either way
  // the sentence is the same one, with "your tool" standing where a name would be: a folder that traces
  // nothing is owed the same account of what the text does as one that traces Claude Code.
  const tool = offer.offered.length === 1 ? offer.offered[0].label : t("agentHook.someTool");

  return (
    <div className="hookconsent__overlay">
      <div className="hookconsent__modal" role="dialog" aria-modal="true" aria-labelledby="agenthook-title">
        <div className="hookconsent__title" id="agenthook-title">{t("agentHook.title")}</div>
        {/* The re-ask leads with what happened since the first one, and is put before — not instead of —
            what the text does: the reader answered this months ago on a screen they no longer have, so a
            panel that only said "you already agreed" would ask for a second yes to something unstated. */}
        {offer.again && <div className="hookconsent__why">{t("agentHook.again")}</div>}
        {/* What a yes buys, in the reader's terms: the text, the hand that makes the edit — theirs, not
            amenbo's — and what their AI does differently afterwards. Said before the buttons, because it
            is the whole of what is being agreed to. */}
        <div className="hookconsent__why">{tf("agentHook.why", { tool })}</div>
        {folders(offer)}

        {error && <div className="hookconsent__error">{error}</div>}

        <div className="hookconsent__actions">
          <button className="hookconsent__action hookconsent__action--yes" disabled={busy} onClick={() => void answer(true)}>
            {t("agentHook.yes")}
          </button>
          <button className="hookconsent__action hookconsent__action--never" disabled={busy} onClick={() => void answer(false)}>
            {t("agentHook.no")}
          </button>
        </div>
        <div className="hookconsent__hint">
          {t(offer.again ? "agentHook.scopeAgain" : "agentHook.scope")} {tf("agentHook.hint", { cmd: offer.cmd })}
        </div>
      </div>
    </div>
  );
}
