// The automations of one project — one screen with three tabs in it.
//
// **One screen, not three entrances.** What a reader does here moves between the three: a definition
// they have just built is the one they want to watch run, and the action a step carries is edited in
// the library and takes effect in every automation using it. Three sidebar entries would make three
// places out of one subject, and each of them would have to carry the way back to the other two.
//
// The tabs are "running" (`AMB-T-5259`), "automations" — this one — and "actions" (`AMB-T-5258`).
// The two that are not built yet draw nothing rather than a line saying so: a placeholder is a
// sentence in nineteen languages that exists to be deleted.
//
// **A definition opens into the build screen.** It is not a pane beside the list: what is being
// looked at is one automation's whole picture, and a list kept beside it would take the width the
// picture needs (`./AutomationBuildScreen`).
import { useState } from "react";
import { AutomationBuildScreen } from "./AutomationBuildScreen";
import { useAutomations } from "../core/automations";
import { t, tf } from "../core/i18n";

/** Which of the three tabs the screen is on. */
type Tab = "running" | "automations" | "actions";

// Spelled out rather than built from the id, so the key gate can see every label a reader can be
// shown (`core/i18n/sourceKeys.test.ts`).
const TABS: readonly { id: Tab; label: () => string }[] = [
  { id: "running", label: () => t("auto.tab.running") },
  { id: "automations", label: () => t("auto.tab.automations") },
  { id: "actions", label: () => t("auto.tab.actions") },
];

export function AutomationsScreen({ projectId }: { projectId: number | null }) {
  const [tab, setTab] = useState<Tab>("automations");
  // Which definition is open, or nothing while the list is. The build screen replaces the list
  // rather than standing beside it, so this is where the screen is and not a selection within it.
  const [open, setOpen] = useState<number | null>(null);
  const automations = useAutomations(projectId);

  if (open !== null) {
    return (
      <AutomationBuildScreen
        id={open}
        projectId={projectId}
        onBack={() => setOpen(null)}
      />
    );
  }

  return (
    <div className="settings">
      <div className="settings__section">
        <div className="settings__body">
          <div className="autotabs" role="tablist" aria-label={t("auto.title")}>
            {TABS.map((one) => (
              <button
                key={one.id}
                type="button"
                role="tab"
                aria-selected={tab === one.id}
                className={`autotabs__tab ${tab === one.id ? "autotabs__tab--on" : ""}`}
                onClick={() => setTab(one.id)}
              >
                {one.label()}
              </button>
            ))}
          </div>

          {tab === "automations" && automations.length === 0 && (
            <div className="auto__empty">{t("auto.empty")}</div>
          )}

          {tab === "automations" && automations.length > 0 && (
            <ul className="auto__list">
              {automations.map((one) => (
                <li key={one.id}>
                  <button type="button" className="auto__row" onClick={() => setOpen(one.id)}>
                    <span className="auto__name">{one.name}</span>
                    {one.archived && <span className="auto__mark">{t("auto.archived")}</span>}
                    <span className="auto__steps">{tf("auto.stepCount", { count: one.steps })}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
