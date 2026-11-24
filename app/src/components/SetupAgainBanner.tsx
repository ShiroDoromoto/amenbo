// The band that asks the reader to set the Viewer up again, put across the whole app.
//
// A Worker older than build 4 cannot take out the rows a run leaves behind on the server, so they stay
// there for good; a build 4 one takes them out, but asking the phones to read again afterwards can cost it
// more than the day allows. Setting up again is what moves the Worker up. That is something the reader can
// do, so the band is the strong one (`healthbanner--offer`, `role="alert"`), the same as the other offers in the stack.
//
// **It says one thing.** No progress of the cleanup and no stretch of time — only that the server needs
// updating and that setting up again does it. The CLI's line on startup says the same.
//
// **Whether it is up is core's whole judgement** (`asks_for_setup_again`); nothing here looks at the build or
// the leftover rows itself. It goes down once core answers `false`, which happens when the setup writes down
// the build it placed, or a send brings a newer build back from the server. A send runs in its own process
// and the row it writes is not on the change feed, so the store watcher asks again whenever the feed comes
// back empty, and while the band is up it also asks again on a timer.
//
// Its button opens the update screen, which asks for the API token and nothing else (`ViewerUpdate`). The
// screen is the shell's to hold, so a band that goes down while it is open does not take it along.
//
// No cross: the band is about the server, not this session, and it goes away by itself once the reason is
// gone. Outside Tauri (in the browser) the answer is always `false`, hence hidden.
import { useEffect } from "react";
import { Icon } from "./Icon";
import { t } from "../core/i18n";
import { recheckSetupAgain, useViewerAsksForSetupAgain } from "../core/viewer";

/** How often the band asks core again while it is up. A local read, so a minute costs next to nothing. */
export const RECHECK_MS = 60_000;

export function SetupAgainBanner({ onUpdate }: { onUpdate: () => void }) {
  const asks = useViewerAsksForSetupAgain();

  useEffect(() => {
    if (!asks) return;
    const timer = setInterval(recheckSetupAgain, RECHECK_MS);
    return () => clearInterval(timer);
  }, [asks]);

  if (!asks) return null;

  return (
    <div className="healthbanner healthbanner--offer" role="alert">
      <Icon name="warning" size="lg" />
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("viewer.setupAgain")}</div>
      </div>
      <button className="healthbanner__action" onClick={onUpdate}>
        {t("viewer.update.open")}
      </button>
    </div>
  );
}
