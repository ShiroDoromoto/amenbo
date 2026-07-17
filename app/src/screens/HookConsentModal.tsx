// The one moment amenbo asks to write into the user's git plumbing, which on this surface is a
// modal. It asks per repository, one at a time, and only ever about repositories core said to ask about
// (`hooks::reconcile`, via `fetchHookOffers`) — nothing here decides whether a question is due.
//
// It is built in-app rather than on the native confirm because the answer has three values and a native
// dialog carries two. "Not now" is not a no: a no is recorded and never asked again, while dismissing
// leaves the project unanswered, which is a state amenbo keeps deliberately (a surface that could not get
// an answer must not invent one). Collapsing the two would turn pressing Esc into "never ask me again".
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

  const offer = offers[0];
  if (!offer) return null;

  // Dropping the head is the whole of "not now": nothing is called, so nothing is recorded, and the next
  // startup finds the same offer waiting.
  const next = () => {
    setError(null);
    setOffers((rest) => rest.slice(1));
  };

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
        <div className="hookconsent__why">{tf("hooks.why", { cmd: `${offer.cmd} lint` })}</div>
        <div className="hookconsent__where">{tf("hooks.where", { project: offer.projectName, dir: offer.dir })}</div>

        {offer.unwired.length > 0 && (
          <div className="hookconsent__slots">
            <div className="hookconsent__slotsTitle">{t("hooks.willWrite")}</div>
            <ul>{offer.unwired.map((slot) => <li key={slot}><code>{slot}</code></li>)}</ul>
          </div>
        )}

        {/* A stranger's hook is never written, so a yes does not cover it — say so next to the line that does. */}
        {offer.foreign.length > 0 && (
          <div className="hookconsent__slots">
            <div className="hookconsent__slotsTitle">{t("hooks.foreign")}</div>
            <ul>
              {offer.foreign.map((slot, i) => (
                <li key={slot}>
                  <code>{slot}</code>
                  <pre className="hookconsent__guidance">{offer.guidance[i]}</pre>
                </li>
              ))}
            </ul>
          </div>
        )}

        {error && <div className="hookconsent__error">{error}</div>}

        <div className="hookconsent__actions">
          <button className="hookconsent__action hookconsent__action--yes" disabled={busy || offer.unwired.length === 0} onClick={() => void answer(true)}>
            {t("hooks.install")}
          </button>
          <button className="hookconsent__action" disabled={busy} onClick={next}>{t("hooks.notNow")}</button>
          <button className="hookconsent__action hookconsent__action--never" disabled={busy} onClick={() => void answer(false)}>
            {t("hooks.never")}
          </button>
        </div>
        <div className="hookconsent__hint">{tf("hooks.hint", { cmd: offer.cmd })}</div>
      </div>
    </div>
  );
}
