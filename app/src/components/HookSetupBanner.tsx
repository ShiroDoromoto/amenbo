import { useEffect, useState } from "react";
import { inTauri } from "../core/snapshot";
import { Icon } from "./Icon";
import { t, tf } from "../core/i18n";
import { fetchHookNotices } from "../core/mutations";
import type { HookNoticeDto } from "../bindings/bindings";
import { DismissButton } from "./DismissButton";

// The GUI's channel for what core's `hooks::setup_notice` found — the same report the CLI puts in its `--json`
// field and on stderr. It tells and stops nothing, and it takes no answer either, which is what keeps it apart
// from the modal that does (`HookConsentModal`). That is also why it carries no install button: consent has one
// surface, and a banner that installed on a click would be writing into the user's git plumbing from a line they
// never answered.
//
// **It is two banners, not one**, because it has two different things to say and they are not degrees of each
// other (core keeps the lists apart for this reason):
//
//   - unwired — the lint is wired to nothing in these slots, empty or held by another tool alike, so the refs
//     it exists to catch are going out uncaught. `hooks install` is the fix — it writes a standalone hook, or
//     slips Amenbo's block in beside another tool's. A warning, and it reads as one. (There is no separate
//     hand-off any more: coexisting is always possible, so a stranger's slot is just a slot to install into.)
//   - restored — a block of ours was found damaged or stale this session and put back (something had changed
//     or removed it — a tool regenerating its hook, a hand-edit). Nothing is unfinished and nothing is asked;
//     it is a heads-up that Amenbo repaired itself, so the reader knows the lint had briefly stopped.
//
// It renders only once the modal is done asking (`asked`), because asking about the hooks and warning about the
// hooks in the same breath says one thing twice. That order is what the notice is read after, too: `hook_offer`'s
// sweep has installed a yes's hooks and healed damaged blocks by then, so `unwired` names only what is still
// missing and `restored` names what the sweep just put back. A recorded "no" and an opted-out repository are
// both silent here (core decides), so this cannot become noise to tune out. Dismissible with the cross for the
// session. Outside Tauri (in the browser) it is always empty, hence hidden.
export function HookSetupBanner({ asked }: { asked: boolean }) {
  const [notices, setNotices] = useState<HookNoticeDto[]>([]);
  // One dismiss per banner: the two say different things, so closing the "restored" heads-up must not also
  // hide the "not wired" warning (and vice versa).
  const [unwiredDismissed, setUnwiredDismissed] = useState(false);
  const [restoredDismissed, setRestoredDismissed] = useState(false);

  useEffect(() => {
    if (!inTauri() || !asked) return;
    let alive = true;
    fetchHookNotices()
      .then((n) => alive && setNotices(n))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show the banner).
    return () => {
      alive = false;
    };
  }, [asked]);

  const unwired = notices.filter((n) => n.unwired.length > 0);
  const restored = notices.filter((n) => n.restored.length > 0);
  const showUnwired = unwired.length > 0 && !unwiredDismissed;
  const showRestored = restored.length > 0 && !restoredDismissed;
  if (!showUnwired && !showRestored) return null;

  return (
    <>
      {showUnwired && (
        <div className="healthbanner healthbanner--offer" role="alert">
          <Icon name="warning" size="lg" />
          <div className="healthbanner__body">
            <div className="healthbanner__title">{t("hookSetup.title")}</div>
            {unwired.map((n) => (
              <div key={n.dir} className="healthbanner__line">
                <div>{tf("hookSetup.where", { project: n.projectName, dir: n.dir })}</div>
                <div>{tf("hookSetup.unwired", { slots: n.unwired.join(", "), cmd: `${n.cmd} hooks install` })}</div>
              </div>
            ))}
          </div>
          <DismissButton onClick={() => setUnwiredDismissed(true)} />
        </div>
      )}
      {showRestored && (
        <div className="healthbanner healthbanner--offer" role="alert">
          <Icon name="warning" size="lg" />
          <div className="healthbanner__body">
            <div className="healthbanner__title">{t("hookRestored.title")}</div>
            {restored.map((n) => (
              <div key={n.dir} className="healthbanner__line">
                <div>{tf("hookSetup.where", { project: n.projectName, dir: n.dir })}</div>
                <div>{tf("hookRestored.slots", { slots: n.restored.join(", ") })}</div>
              </div>
            ))}
          </div>
          <DismissButton onClick={() => setRestoredDismissed(true)} />
        </div>
      )}
    </>
  );
}
