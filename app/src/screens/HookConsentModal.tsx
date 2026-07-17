// The one moment amenbo asks to write into the user's git plumbing, which on this surface is a
// modal. It asks per repository, one at a time, and only ever about repositories core said to ask about
// (`hooks::reconcile`, via `fetchHookOffers`) — nothing here decides whether a question is due.
//
// It is a consent gate, not a consultation. Nobody wants an AMB-T-… in their commits, so there is no
// judgement to delegate: the question is whether amenbo may write, and the answers are yes and no. What it
// takes to honour a yes — an empty slot, a hooks directory the whole team shares, a stranger already in the
// other slot — is core's to work out (`hooks::install`), and none of it is put on screen. Core only ever
// asks when a yes could write something, so the yes button is never dead.
//
// The answer still has three values against the modal's two buttons. "Not now" is not a no: a no is
// recorded and never asked again, while dismissing leaves the project unanswered, which is a state amenbo
// keeps deliberately (a surface that could not get an answer must not invent one). Esc is that third value,
// which is why it must not be wired to `answer(false)` — pressing it would then mean "never ask me again".
import { useEffect, useState } from "react";
import { answerHookOffer, fetchHookOffers } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { HookOfferDto } from "../bindings/bindings";

/**
 * `onDone` fires once this has nothing left to ask — every offer answered or waved past, or there was
 * never one. It is what lets the setup banner wait its turn: the banner reports what is still unwired,
 * and asking someone about a repository while warning them about the same repository says one thing
 * twice. A failure to fetch counts as done, so a surface that could not ask does not also mute the one
 * that only tells.
 */
export function HookConsentModal({ onDone }: { onDone?: () => void }) {
  const [offers, setOffers] = useState<HookOfferDto[]>([]);
  const [asked, setAsked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    fetchHookOffers()
      .then((o) => alive && setOffers(o))
      .catch(() => {}) // A failure to detect is swallowed: we ask nothing rather than block the app.
      .finally(() => alive && setAsked(true));
    return () => {
      alive = false;
    };
  }, []);

  // Reported once the fetch has landed, so "nothing to ask" is never confused with "not asked yet". It can
  // fire more than once (the last offer leaving, then a re-render), so `onDone` must be idempotent.
  useEffect(() => {
    if (asked && offers.length === 0) onDone?.();
  }, [asked, offers.length, onDone]);

  // Dropping the head is the whole of "not now": nothing is called, so nothing is recorded, and the next
  // startup finds the same offer waiting.
  const next = () => {
    setError(null);
    setOffers((rest) => rest.slice(1));
  };

  // Esc is "not now", and it is the only way to reach that answer now the button for it is gone. It stays
  // deliberately unbound to no: dismissing a dialog is how people put a question off, and recording that as
  // a refusal would answer on their behalf — the one thing this surface must not do.
  //
  // Above the early return because a hook cannot be reached conditionally; the listener is harmless with no
  // question up, since dropping the head of an empty list is a no-op.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) next();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const offer = offers[0];
  if (!offer) return null;

  const answer = async (yes: boolean) => {
    setBusy(true);
    try {
      await answerHookOffer(offer.projectId, offer.dir, yes);
      next();
    } catch (e) {
      setError(errText(e)); // The install failed, so nothing was recorded: leave the question up.
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="hookconsent__overlay">
      <div className="hookconsent__modal" role="dialog" aria-modal="true" aria-labelledby="hookconsent-title">
        <div className="hookconsent__title" id="hookconsent-title">{t("hooks.title")}</div>
        <div className="hookconsent__why">{t("hooks.why")}</div>
        {/* Which repository, because the offers come one per project and answering four in a row is
            otherwise four identical questions. This is the only fact from the probe that reaches the
            user, and it is identity, not plumbing. */}
        <div className="hookconsent__where">{tf("hooks.where", { project: offer.projectName, dir: offer.dir })}</div>

        {error && <div className="hookconsent__error">{error}</div>}

        <div className="hookconsent__actions">
          <button className="hookconsent__action hookconsent__action--yes" disabled={busy} onClick={() => void answer(true)}>
            {t("hooks.yes")}
          </button>
          <button className="hookconsent__action hookconsent__action--never" disabled={busy} onClick={() => void answer(false)}>
            {t("hooks.no")}
          </button>
        </div>
        <div className="hookconsent__hint">{tf("hooks.hint", { cmd: offer.cmd })}</div>
      </div>
    </div>
  );
}
