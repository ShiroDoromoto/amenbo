// The question about having a folder's AI read amenbo's instruction at the start of every session
// (`AMB-D-440`), which on this surface is a modal. It is the GUI's half of the CLI's `offer_agent_hook`.
//
// **It is asked about a state, not put on the way to creating something.** What raises it is a bound folder
// that is not wired, found on the startup sweep — never the project-creation walk. A question asked while a
// project is being made would reach the projects made from here on and miss every one that already exists,
// which is most of them; asking about the state reaches both.
//
// **The answer is per project**, unlike the lint's one-per-device consent: whether an AI is trusted to be
// started on amenbo here is a question whose answer genuinely changes with the place, so the modal names
// the project and hands the answer back with it.
//
// **A yes wires nothing.** amenbo does not write into a user's provider settings — it hands over a text for
// an AI of theirs to act on — so the whole of what a yes buys is the text, and the setup banner is where it
// arrives (with the copy button). That is also why the record cannot end the banner: only the edit landing does.
//
// Three values, two buttons, exactly as `HookConsentModal` has them: dismissing is "not now" and records
// nothing, which is why Esc must not be wired to `answer(false)` — that answer means "never ask again".
import { useEffect, useState } from "react";
import { answerAgentHookOffer, fetchAgentHookOffer } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { AgentHookOfferDto } from "../bindings/bindings";

/**
 * `turn` is the one-question-at-a-time rule: false while the lint's modal is still asking, and nothing here
 * is fetched or shown until it goes true. `canAsk` is the rest of that rule — false when the lint actually
 * put its question this run, in which case the startup sweep still runs (a folder wired by hand is adopted
 * without anyone being asked) but no question is raised, and this one comes round at a later startup.
 *
 * `onDone` fires once there is nothing left to ask — answered, waved past, or never there — which is what
 * lets the setup banner wait its turn. A failure to fetch counts as done, so a surface that could not ask
 * does not also mute the one that only tells. It can fire more than once, so it must be idempotent.
 */
export function AgentHookConsentModal({ turn, canAsk, onDone }: {
  turn: boolean;
  canAsk: boolean;
  onDone?: () => void;
}) {
  const [offer, setOffer] = useState<AgentHookOfferDto | null>(null);
  const [asked, setAsked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!turn) return;
    let alive = true;
    fetchAgentHookOffer(canAsk)
      .then((o) => alive && setOffer(o))
      .catch(() => {}) // A failure to detect is swallowed: we ask nothing rather than block the app.
      .finally(() => alive && setAsked(true));
    return () => {
      alive = false;
    };
    // `canAsk` is settled by the time `turn` goes true and never moves after, so the probe runs once.
  }, [turn, canAsk]);

  useEffect(() => {
    if (asked && !offer) onDone?.();
  }, [asked, offer, onDone]);

  // Dropping the question is the whole of "not now": nothing is called, so nothing is recorded, and a later
  // startup finds it waiting.
  const dismiss = () => {
    setError(null);
    setOffer(null);
  };

  // Esc is "not now", and deliberately not wired to a no — dismissing a dialog is how people put a question
  // off, and recording that as a refusal would answer on their behalf. Above the early return because a hook
  // cannot be reached conditionally; harmless with no question up.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!offer) return null;

  const answer = async (yes: boolean) => {
    setBusy(true);
    try {
      await answerAgentHookOffer(offer.projectId, yes);
      setOffer(null);
    } catch (e) {
      setError(errText(e)); // Nothing was recorded: leave the question up.
    } finally {
      setBusy(false);
    }
  };

  // Named only where the folder points at exactly one tool. With several, which one is the reader's to say,
  // and the banner lists them all afterwards; with none, there is nothing to name. Either way the sentence
  // is the same one, with "your tool" standing where a name would be — a folder that traces nothing is
  // owed the same account of what the text does as one that traces Claude Code.
  const tool = offer.named.length === 1 ? offer.named[0].label : t("agentHook.someTool");

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
        <div className="hookconsent__scope">{tf("agentHook.where", { project: offer.projectName, dir: offer.dir })}</div>

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
