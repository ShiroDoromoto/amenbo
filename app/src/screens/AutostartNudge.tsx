// "You have been opening Amenbo a while — shall it open itself when you sign in?" (`AMB-D-545`).
//
// The question is deliberately late. At the first launch nobody has decided to keep Amenbo yet, and a
// no given then is kept: the setting would read "off" forever with nothing ever asking again. So the
// thresholds in core hold it until the app has been come back to (`crate::nudge`), and this only words
// what that judgement released.
//
// It is put on the ON side, because by the time it appears the person is opening Amenbo most days and
// the offer is one fewer thing for them to do. A no costs nothing and is not final either — Settings ›
// Startup carries the same switch, which is what the hint says.
import { useState } from "react";
import { setAutostart, fetchDevBadge } from "../core/mutations";
import { getSnapshot } from "../core/snapshot";
import { errText, t } from "../core/i18n";

/**
 * Whether this surface has the login registration to offer at all — the stage core's declaration is
 * held behind (`autostart_offerable`, and the name has to match the one in `crate::nudge`).
 *
 * Two things, both of them this side's to know: a development build registers nothing at login
 * (`AMB-D-547`), and a setting already on has nothing left to ask about. Either one closes the stage,
 * and so does a failure to find out which build this is — an unanswered question is not put.
 */
export async function autostartOfferable(): Promise<boolean> {
  if (getSnapshot().autostart) return false;
  return (await fetchDevBadge()) === null;
}

/**
 * The modal the nudge is put as. `onClose` takes it off the screen — it is put once, so whichever way
 * it goes, it goes for good.
 */
export function AutostartNudge({ onClose }: { onClose: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // A yes is the setting being turned on, through the one door that writes both halves of it: the OS
  // registration first, and `config.autostart` only if that landed. A failure keeps the question up
  // rather than closing on a login that was never registered.
  const yes = async () => {
    setBusy(true);
    setError(null);
    try {
      await setAutostart(true);
      onClose();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  // A no writes nothing: off is what the setting already says, and saying it again would only risk an
  // error over a value that is not changing. What keeps the question from coming back is the log core
  // wrote when this was put, not anything recorded here.
  return (
    <div className="modal__overlay">
      <div className="nudge__modal" role="dialog" aria-modal="true" aria-labelledby="nudge-autostart-title">
        <div className="nudge__title" id="nudge-autostart-title">{t("nudge.autostart.title")}</div>
        {/* The settings row's own sentence, because it is the same offer said to the same person — one
            explanation to keep true, and one already carried in every language. */}
        <div className="nudge__why">{t("settings.autostartNote")}</div>

        {error && <div className="nudge__error">{error}</div>}

        <div className="buttonrow">
          <button className="btn btn--primary" disabled={busy} onClick={() => void yes()}>
            {t("nudge.autostart.yes")}
          </button>
          <button className="btn" disabled={busy} onClick={onClose}>
            {t("nudge.autostart.no")}
          </button>
        </div>
        <div className="nudge__hint">{t("nudge.autostart.hint")}</div>
      </div>
    </div>
  );
}
