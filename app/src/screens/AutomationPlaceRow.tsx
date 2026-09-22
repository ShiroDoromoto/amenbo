// **Put an action on the picture**, under the picture on the build screen's "build" place.
//
// **It is the road the `+` on a line is not.** That one joins a picture already drawn, and the first
// box has no line to join — so an automation born with nothing on it would have no way in at all. It
// stands under the picture rather than only in the empty frame, because the second box does not
// always belong on a line either.
//
// **A row and not a dialog, for the action already on the shelf.** What that takes is one answer —
// which action stands here — and the spot it makes is then edited in the panel like any other
// (`./AutomationStepPanel`). A modal for one pulldown would be a window in front of the picture the
// reader is placing onto.
//
// **Writing one here is the row's second press, and that one opens the dialog**
// (`./AutomationStepAdd`, `AMB-T-5317`). What it takes is a whole action — a name, a prompt, the
// library to keep it in, the ways out and the inputs — which is more than a row holds. It stands
// beside the pulldown rather than behind an empty library, because writing the words is not a
// fallback for having none on the shelf: a build screen is where a one-off action is written
// (`AMB-D-949`).
//
// **The pulldown goes when the library is empty and the press stays**, so a project with nothing on
// its shelf still has its way onto the picture.
import { useState } from "react";
import { placeAutomationAction, useAutomationActions } from "../core/automations";
import { errText, t } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { AutomationStepAdd } from "./AutomationStepAdd";

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
  const [writing, setWriting] = useState(false);

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
      {actions.length === 0 && <div className="autostep__said">{t("auto.pic.libraryEmpty")}</div>}
      <div className="autostep__declare">
        {actions.length > 0 && (
          <>
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
          </>
        )}
        <button type="button" className="btn" onClick={() => setWriting(true)}>
          {t("auto.pic.writeDo")}
        </button>
      </div>
      {writing && (
        <AutomationStepAdd
          into={{ picture: "automation", automationId }}
          projectId={projectId}
          // Nothing comes before this box, so nothing says who should carry it out: the dialog
          // starts on the same agent an action's first step does, and the panel is where it changes.
          agent="claude-code"
          onClose={() => setWriting(false)}
        />
      )}
    </div>
  );
}
