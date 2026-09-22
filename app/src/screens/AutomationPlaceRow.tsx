// **Put an action on the picture**, under the picture on the build screen's "build" place.
//
// **It is the road the `+` on a line is not.** That one joins a picture already drawn, and the first
// box has no line to join — so an automation born with nothing on it would have no way in at all. It
// stands under the picture rather than only in the empty frame, because the second box does not
// always belong on a line either.
//
// **A row and not a dialog.** What it takes is one answer — which action stands here — and the spot
// it makes is then edited in the panel like any other (`./AutomationStepPanel`). A modal for one
// pulldown would be a window in front of the picture the reader is placing onto.
//
// **Only the library is offered.** Writing an action where the reader is looking belongs to the
// dialog the `+` opens (`./AutomationStepAdd`), which has the line to put one in front of; a project
// whose library is empty is sent to the actions tab, where one is made.
import { useState } from "react";
import { placeAutomationAction, useAutomationActions } from "../core/automations";
import { errText, t } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";

export function AutomationPlaceRow({
  automationId,
  projectId,
}: {
  automationId: number;
  /** Whose library is reached — the project's own and the device's, in one list. */
  projectId: number | null;
}) {
  const actions = useAutomationActions(projectId);
  const [picked, setPicked] = useState("");
  const [refused, setRefused] = useState<string | null>(null);

  if (actions.length === 0) {
    return <div className="autostep__said">{t("auto.pic.libraryEmpty")}</div>;
  }

  // Core refuses an action another project's library holds, and the sentence it writes is what the
  // row draws — the pulldown offers what is within reach, so this is the list having moved under it.
  const place = () => {
    setRefused(null);
    void placeAutomationAction(automationId, Number(picked)).catch((e: unknown) =>
      setRefused(errText(e)),
    );
  };

  return (
    <div className="autostep__field">
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}
      <span className="autostep__label">{t("auto.pic.place")}</span>
      <div className="autostep__declare">
        <select
          aria-label={t("auto.pic.place")}
          value={picked}
          onChange={(e) => setPicked(e.target.value)}
        >
          <option value="">—</option>
          {actions.map((one) => (
            <option key={one.id} value={String(one.id)}>
              {one.name}
            </option>
          ))}
        </select>
        <button type="button" className="btn" disabled={picked === ""} onClick={place}>
          {t("auto.pic.placeDo")}
        </button>
      </div>
    </div>
  );
}
