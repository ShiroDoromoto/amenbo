// The one moment amenbo asks to write into the user's git plumbing, which on this surface is a modal.
// **It is one question, once on this device** — not one per repository. Whether it is due at all is
// core's to say (`hooks::reconcile`, via `fetchHookOffer`); nothing here decides that.
//
// It is a consent gate, not a consultation. Nobody wants an AMB-T-… in their commits, so there is no
// judgement to delegate: the question is whether amenbo may wire the lint, and the answers are yes and no.
// What it takes to honour a yes — which repositories are bound, an empty slot, a hooks directory the whole
// team shares, a stranger already in the other slot — is core's to work out (`hooks::install`), and none of
// it is put on screen. Core only ever asks when a yes could write something, so the yes button is never dead.
//
// Nor does it list the repositories the answer covers. That would turn one question into a page to read and
// check, which is the cost the one-question design set out to remove — and it would be offering a
// per-repository choice the answer does not have. A repository that wants out says so afterwards, with
// `hooks uninstall`.
//
// The answer still has three values against the modal's two buttons. "Not now" is not a no: a no is
// recorded and never asked again, while dismissing leaves the device unanswered, which is a state amenbo
// keeps deliberately (a surface that could not get an answer must not invent one). Esc is that third value,
// which is why it must not be wired to `answer(false)` — pressing it would then mean "never ask me again".
import { useEffect, useState } from "react";
import { answerHookOffer, fetchHookOffer } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { HookOfferDto } from "../bindings/bindings";

/**
 * `onDone` fires once this has nothing left to ask — the question answered or waved past, or there was
 * never one. It is what lets the setup banner wait its turn: the banner reports what is still unwired,
 * and asking someone about the hooks while warning them about the hooks says one thing twice. A failure to
 * fetch counts as done, so a surface that could not ask does not also mute the one that only tells.
 */
export function HookConsentModal({ onDone }: { onDone?: () => void }) {
  const [offer, setOffer] = useState<HookOfferDto | null>(null);
  const [asked, setAsked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    fetchHookOffer()
      .then((o) => alive && setOffer(o))
      .catch(() => {}) // A failure to detect is swallowed: we ask nothing rather than block the app.
      .finally(() => alive && setAsked(true));
    return () => {
      alive = false;
    };
  }, []);

  // Reported once the fetch has landed, so "nothing to ask" is never confused with "not asked yet". It can
  // fire more than once (the question leaving, then a re-render), so `onDone` must be idempotent.
  useEffect(() => {
    if (asked && !offer) onDone?.();
  }, [asked, offer, onDone]);

  // Dropping the question is the whole of "not now": nothing is called, so nothing is recorded, and the
  // next startup finds it waiting.
  const dismiss = () => {
    setError(null);
    setOffer(null);
  };

  // Esc is "not now", and it is the only way to reach that answer now the button for it is gone. It stays
  // deliberately unbound to no: dismissing a dialog is how people put a question off, and recording that as
  // a refusal would answer on their behalf — the one thing this surface must not do.
  //
  // Above the early return because a hook cannot be reached conditionally; the listener is harmless with no
  // question up, since clearing an already-clear question is a no-op.
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
      await answerHookOffer(yes);
      setOffer(null);
    } catch (e) {
      setError(errText(e)); // Nothing was recorded: leave the question up.
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="hookconsent__overlay">
      <div className="hookconsent__modal" role="dialog" aria-modal="true" aria-labelledby="hookconsent-title">
        <div className="hookconsent__title" id="hookconsent-title">{t("hooks.title")}</div>
        <div className="hookconsent__why">{t("hooks.why")}</div>
        {/* That the answer is asked once and covers the repositories amenbo works in — said, because it is
            what clicking does, and a question that collected a wider consent than it admitted to would be
            the wrong kind of quiet. Said in one line, and without naming them: see the header. */}
        <div className="hookconsent__scope">{t("hooks.scope")}</div>

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
