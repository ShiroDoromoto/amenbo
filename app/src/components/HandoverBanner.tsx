// Where the plugins went, said once in this window (`AMB-D-884`).
//
// A person who installed mail, slack, viewer or worktree and finds the whole Plugins screen gone is owed
// the sentence that says it is part of Amenbo now — and someone who only ever opens the app never meets
// the command line's version of it. The migration that took them in left the account for both surfaces
// to read, each marked told on its own.
//
// **A band, not a modal.** Nothing is being asked: there is no button whose absence leaves the app in a
// wrong state, and the app's own line is that a modal is for a question and a band for a statement
// (`HookConsentModal` against `HookSetupBanner`). It is also the launch a person is most likely to meet
// the hooks question on, and stacking a second dialog in front of that one would bury both.
//
// **It wears the accent, not the warning colour.** This is news and not a fault, so it is drawn like the
// other things the app offers rather than like the ones it is worried about.
//
// **Putting it away is what takes the turn.** The write is what records it, so a band dismissed over a
// store that refused the write stays up with the reason on it — one that vanished would leave the reader
// believing they had been told something the store does not know they were.
import { useEffect, useState } from "react";
import { inTauri } from "../core/snapshot";
import { fetchHandoverNotice, markHandoverTold } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { HandoverDto } from "../bindings/bindings";
import { DismissButton } from "./DismissButton";
import { ErrorNote } from "./ErrorNote";
import { Icon } from "./Icon";

export function HandoverBanner() {
  const [said, setSaid] = useState<HandoverDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    fetchHandoverNotice()
      .then((held) => alive && setSaid(held))
      .catch(() => {}); // An account that could not be read is not a band to put up.
    return () => {
      alive = false;
    };
  }, []);

  if (!said) return null;

  const dismiss = async () => {
    setBusy(true);
    setError(null);
    try {
      await markHandoverTold();
      setSaid(null);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="healthbanner healthbanner--offer" role="status">
      <Icon name="plug" size="lg" />
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("handover.title")}</div>
        <div className="healthbanner__line">
          {tf("handover.plugins", { plugins: said.plugins.join(", ") })}
        </div>
        {said.targets > 0 && (
          <div className="healthbanner__line">
            {tf("handover.notify", { targets: String(said.targets), projects: String(said.projects) })}
          </div>
        )}
        {said.viewer && <div className="healthbanner__line">{t("handover.viewer")}</div>}
        {said.plugins.includes("worktree") && (
          <div className="healthbanner__line">{t("handover.worktree")}</div>
        )}
      </div>
      {error && <ErrorNote>{error}</ErrorNote>}
      <DismissButton onClick={dismiss} disabled={busy} />
    </div>
  );
}
