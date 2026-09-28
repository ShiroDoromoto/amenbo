// An action's build screen, opened over the automation it was reached from (`AMB-D-1004`).
//
// **It stands over the automation rather than in its place.** Going to the action's own screen turned
// the tab to "actions", and the "automations" tab then landed on the list — the automation somebody
// was building was nowhere to be pressed back to. Over it, the automation stays where it was, with the
// box that was pressed still pressed.
//
// **Only its own back closes it.** The backdrop covers the tabs, the sidebar and "＜" "＞", and neither
// a press outside nor Escape closes it: a reader half way through building an action should not lose
// their place to a stray press. "Open full screen" is the other way out, to the action's own screen,
// whose back lands on the automation again.
//
// **Its panel stands in its own column.** The screen draws its panel into the shell's right-pane
// column (`../shell/paneSlot`), which is behind the backdrop here — so this lends it one of its own,
// shown only while a panel claims it, the way the shell's is.
//
// **Nothing in it goes somewhere else.** The names of the automations the action is placed on and the
// runs holding it are read here, not pressed: a press would take the screen away from under the reader
// by a door other than the back.
import { useCallback, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { AutomationActionBuildScreen } from "./AutomationActionBuildScreen";
import { PaneSlotProvider } from "../shell/paneSlot";
import { t, tf } from "../core/i18n";

export function AutomationActionOver({
  actionId,
  automationName,
  boxNo,
  onBack,
  onFull,
}: {
  actionId: number;
  /** The automation under it — what its back is named after. */
  automationName: string;
  /** The number the automation's picture gives the box this action is placed in, once it is there. */
  boxNo?: number;
  onBack: () => void;
  /** Go to the action's own screen instead, with the automation under it to come back to. */
  onFull: () => void;
}) {
  const [claims, setClaims] = useState(0);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const claim = useCallback(() => {
    setClaims((n) => n + 1);
    return () => setClaims((n) => n - 1);
  }, []);
  const pane = useMemo(() => ({ slot, claim }), [slot, claim]);

  return createPortal(
    <div className="modal__overlay">
      <div className="actover" role="dialog" aria-modal="true">
        <PaneSlotProvider value={pane}>
          <div className="actover__main">
            <AutomationActionBuildScreen
              id={actionId}
              onBack={onBack}
              backLabel={tf("auto.over.back", { name: automationName })}
              headLead={
                boxNo !== undefined && (
                  <span className="actover__where">
                    <span className="autopic__no">{boxNo}</span> {t("auto.over.placed")}
                  </span>
                )
              }
              headEnd={
                <button type="button" className="btn actover__full" onClick={onFull}>
                  {t("auto.over.full")}
                  <span aria-hidden="true"> ↗</span>
                </button>
              }
            />
          </div>
          <div className="actover__pane" ref={setSlot} hidden={claims === 0} />
        </PaneSlotProvider>
      </div>
    </div>,
    document.body,
  );
}
