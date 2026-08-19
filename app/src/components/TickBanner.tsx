// The offer to let amenbo be woken once an hour, put across the whole app (`AMB-D-718`). Until it is
// answered the due warning is silent, and nothing anywhere says so — `amenbo tick install` is the only
// other way in, and a reader who has not met the command line has no way of learning it exists.
//
// **It is the app's question, not a project's.** One machine holds one timer, so there is nothing
// narrower for the answer to speak for, and a board is the wrong place to put a question about the
// device. It goes in the stack of bands, beside the other things the app has to say about itself.
//
// **It looks like the standing row on a project's screen** (`AgentHookWiringRow`), and is drawn in the
// same box: the app's own surface, an accent edge down its side. Not a filled band — this one is up on
// every launch for as long as the conditions hold, and a colour spent on something seen daily is a
// colour that no longer says "stop" when the store is in trouble.
//
// **Three buttons, each named after what pressing it leaves behind** (`AMB-D-663`):
//
//   - start checking — the answer is yes and the timer is registered. The registration is written first,
//     so a scheduler that refused leaves the question open rather than a config claiming a timer.
//   - don't show this again — the answer is no. Nothing is registered, and the settings screen is the
//     way back.
//   - later — no answer at all, and the day is written down (`tick_banner_later`). Tomorrow it is asked
//     again. It is the only one of the three that leaves the question live.
//
// Whether it is up at all is core's whole judgement (`tick::banner_shows`), read once at startup: the
// answer it turns on is the device's, and the pass that settles it against the scheduler has already run
// by the time this mounts.
import { useEffect, useState } from "react";
import { answerTick, deferTickBanner, fetchTickBanner } from "../core/mutations";
import { errText, t } from "../core/i18n";
import { ErrorNote } from "./ErrorNote";
import { Icon } from "./Icon";

export function TickBanner() {
  const [shows, setShows] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    fetchTickBanner()
      .then((up) => alive && setShows(up))
      .catch(() => {}); // A judgement that could not be made is not a question to put.
    return () => {
      alive = false;
    };
  }, []);

  if (!shows) return null;

  // Every button ends the banner for this run of the app; what differs is what it left on disk. A
  // failure is not swallowed — the banner stays up with the reason on it, because one that vanished on a
  // write that never landed would report an answer nobody has.
  const press = (write: () => Promise<void>) => async () => {
    setBusy(true);
    setError(null);
    try {
      await write();
      setShows(false);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="standingrow tickbanner">
      <div className="tickbanner__title"><Icon name="clock" size="md" /> {t("tickBanner.title")}</div>
      <div className="tickbanner__what">{t("tickBanner.what")}</div>

      {error && <ErrorNote>{error}</ErrorNote>}

      <div className="tickbanner__actions">
        <button className="btn btn--primary" disabled={busy} onClick={press(() => answerTick(true))}>
          {t("tickBanner.start")}
        </button>
        <button className="btn" disabled={busy} onClick={press(() => answerTick(false))}>
          {t("tickBanner.never")}
        </button>
        {/* Its own label, not the shared "close" one: the three differ in what they leave behind, and
            that is what each says (`AMB-D-663`). */}
        <button className="btn" disabled={busy} onClick={press(deferTickBanner)}>
          {t("tickBanner.later")}
        </button>
      </div>
    </div>
  );
}
