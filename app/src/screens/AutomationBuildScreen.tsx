// One automation, opened — the screen it is built and started from.
//
// **It is laid out the way an action's build screen is** (`./AutomationActionBuildScreen`): a head
// with the way back and the name, a band over the picture, and the picture with a panel pinned to its
// right. The two screens are one move a layer apart (`AMB-D-949`), and a reader going between them
// should not have to learn a second arrangement.
//
// **Three places, each named on the screen**: "launch", the band that refuses; "build", the picture of
// the placements (`./AutomationPicture`); and "placement", what the pressed spot holds
// (`./AutomationStepPanel`), which is the panel's reading of a press on a box.
//
// **Everything else the screen asks is the panel too.** A `+` on a line, or the press on an empty
// picture, opens the library in it (`./AutomationLibraryPanel`); "Edit" on the head opens the
// definition's own name, notes and archiving, with the press that deletes it
// (`./AutomationAboutPanel`). They are one panel rather than four places stacked under the picture:
// a picture that runs long would push whatever is under it off the bottom of the window, and the
// panel stands where the reader is looking however far they scrolled. Only the picture scrolls.
//
// **The head leads with the automation's ID**, as the list's rows do: it is what the terminal names
// the definition by, and it does not change when the name does.
//
// **A dialog opens for one thing only: making an action on the spot** (`./AutomationActionMake`).
// Picking one off the shelf is done beside the picture, where the line it goes on can still be seen.
// The dialog asks a name and a library, puts the empty action where it was asked for, and the screen
// goes on to that action's own build screen, where its inside is built (`AMB-D-956`).
//
// **What stands on the picture is a placement of a library action** (`AMB-D-949`). Nothing here
// writes a step: a step is inside the action, and the screen that draws those is the action's own.
//
// **The delete takes the screen with it**, so the press hands back the same way out the "back"
// button does: there is no definition left for this screen to be drawn from.
//
// **An automation a run is going on is read, not written, for as long as the run goes** (`AMB-D-961`).
// Core refuses every rewrite of it while a run of it is running or paused, so nothing adds to the
// picture, the panels open to be read with every write in them held shut, and the runs are named over
// the picture with the way to each one's pane (`./AutomationHeldBy`). Starting another run is not a
// rewrite, and stays.
//
// **What the panel shows is the screen's, not the picture's.** The picture marks the pressed box and
// the panel draws it, so it is held where both can see it, and the panel hands it back when the spot
// it was drawn from is taken off.
//
// **The launch place refuses, the build place never does.** Building is always half-finished — a step
// with no way onward, an input nobody has wired — and every one of those saves
// (`amenbo_core::ops::automation`). What is unfinished only matters at the moment somebody is about
// to be let down by it, which is here: the reasons are listed, and the button is not offered while
// there is one.
//
// **The press can still be refused, and its refusal is drawn where the list is.** Two things move
// between the screen being drawn and the button being pressed — the machine, and whether the
// workspace is standing — and core raises both as a sentence rather than as a reason on the list
// (`amenbo_core::ops::automation_run::launch`). A run that took no lane is not a refusal: it says so
// and waits.
//
// **The list of reasons is read, not worked out here** (`../core/automations`). It was worked out
// here once, beside core's own, and the two disagreed on five points — so the screen could say an
// automation was ready and the press then refuse it (`AMB-T-5272`).
//
// **A closed workspace is not on the list.** It is not about the definition and stops being true the
// moment a window opens, so the launch raises it at the press rather than the build screen drawing it
// among things somebody has to go and fix (`amenbo_core::ops::automation_run::launch`). Whether it is
// standing is handed down from the shell, which is the one place that knows which window holds it.
import { useState } from "react";
import { AutomationAboutPanel } from "./AutomationAboutPanel";
import { Panel } from "./AutomationActionBuildScreen";
import { AutomationLibraryPanel, type PlaceTarget } from "./AutomationLibraryPanel";
import { AutomationPicture } from "./AutomationPicture";
import { automationGraph } from "./automationLayout";
import { AutomationActionMake } from "./AutomationActionMake";
import { AutomationHeldBy } from "./AutomationHeldBy";
import { AutomationStepPanel } from "./AutomationStepPanel";
import type { WhereTo } from "./automationParts";
import { useAutomationStart } from "../components/StartAutomation";
import { useAutomation, useLaunchCheck } from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { errSentence, t, tf } from "../core/i18n";
import { builtinWord } from "../core/builtinWords";
import { Icon } from "../components/Icon";
import type { AutomationDetailDto } from "../bindings/bindings";

/** Where the library's pick will go: after which way out of which box, or first of all. */
function whereTo(automation: AutomationDetailDto | null, target: PlaceTarget): WhereTo {
  if (!("edgeId" in target)) return null;
  const edge = automation?.edges.find((one) => one.id === target.edgeId);
  const from = automation?.placements.find((one) => one.id === edge?.fromId);
  const box = from === undefined ? "" : builtinWord(from.builtin, from.name);
  return { box, exit: edge?.exitName, builtin: from?.builtin };
}

/** What the panel is showing, if anything. */
type Showing =
  | { kind: "box"; id: number }
  | { kind: "library"; target: PlaceTarget }
  | { kind: "about" };

export function AutomationBuildScreen({
  id, projectId, workspaceOpen, onBack, onOpenAction, onGoToRun,
}: {
  id: number;
  /** Whose project this automation is — what the machine is asked about, and where its folders are. */
  projectId: number | null;
  /**
   * Whether the workspace is standing. It is handed down rather than asked here: which window holds
   * it is the shell's to know (`../shell/AppShell`, `AMB-D-753`), and core refuses a launch without
   * it — last of its three refusals, so a half-built automation is named before a window is.
   */
  workspaceOpen: boolean;
  onBack: () => void;
  /** Go to one library action's own build screen — where an action made here is built. */
  onOpenAction: (actionId: number) => void;
  /** Go to the pane a run holding this automation is drawn in. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  const automation = useAutomation(id);
  const held = (automation?.heldBy.length ?? 0) > 0;
  // Nothing until something is pressed — a definition opens on the picture, and a box picked for the
  // reader would be one they did not choose.
  const [showing, setShowing] = useState<Showing | null>(null);
  // Where the dialog that makes an action on the spot is about to put it, while it is open.
  const [making, setMaking] = useState<PlaceTarget | null>(null);
  const folders = useBoundFolders(projectId);
  const check = useLaunchCheck(id, projectId, folders.live.map((one) => one.path));
  // The press itself is the one every entrance makes (`../components/StartAutomation`): this screen
  // is where an automation is built, not a third place for a launch to behave differently.
  const { start, refused, starting } = useAutomationStart(projectId, workspaceOpen, onGoToRun);

  const pressed =
    showing?.kind === "box"
      ? automation?.placements.find((one) => one.id === showing.id) ?? null
      : null;
  const close = () => setShowing(null);

  return (
    <div className="actbuild">
      <div className="actbuild__head">
        <button type="button" className="btn" onClick={onBack}>
          <Icon name="chevronLeft" /> {t("auto.build.back")}
        </button>
        {automation !== null && (
          <span className="autoid">{tf("auto.id", { id: automation.id })}</span>
        )}
        <span className="actbuild__name">{automation?.name ?? ""}</span>
        <button
          type="button"
          className={showing?.kind === "about" ? "btn btn--on actbuild__edit" : "btn actbuild__edit"}
          aria-pressed={showing?.kind === "about"}
          // It opens and does not toggle: a second press while the panel stands is somebody meaning
          // to be there, and the panel's own × is the way out of it.
          onClick={() => setShowing({ kind: "about" })}
        >
          {t("auto.build.edit")}
        </button>
      </div>

      <div className="autolaunch">
        <span className="actbuild__sec">{t("auto.build.launch")}</span>

        {check === null && <div className="auto__empty">{t("app.loading")}</div>}

        {check !== null && (
          <div className={check.ready ? "autolaunch__box autolaunch__box--ok" : "autolaunch__box autolaunch__box--no"}>
            <div className="autolaunch__title">
              {check.ready ? (
                <span className="auto__ready">{t("auto.ready")}</span>
              ) : (
                <span className="auto__notready">{t("auto.notReady")}</span>
              )}
              <button
                type="button"
                className="btn btn--primary autolaunch__start"
                disabled={!check.ready || starting || projectId === null}
                onClick={() => void start(id, folders.live.map((one) => one.path))}
              >
                {t("auto.start")}
              </button>
            </div>
            {!check.ready && (
              <ul className="auto__blocks">
                {/* Each reason names itself, so the sentence comes from the same place the press's
                    refusal writes its own from (`core/i18n`'s `errSentence`) — this list and that one
                    are the same words, and holding them apart is what let them drift. */}
                {check.blocks.map((block, nth) => (
                  <li key={`${block.code}-${nth}`}>{errSentence(block)}</li>
                ))}
              </ul>
            )}
          </div>
        )}

        {refused !== null && <div className="auto__notready">{refused}</div>}
      </div>

      {automation !== null && <AutomationHeldBy runs={automation.heldBy} onGoToRun={onGoToRun} />}

      <div className="actbuild__canvashead">
        <span className="actbuild__sec">{t("auto.build.picture")}</span>
      </div>
      <div className="actbuild__canvas">
        <AutomationPicture
          graph={automationGraph(automation)}
          selectedBoxId={pressed?.id}
          onPickBox={(box) => setShowing({ kind: "box", id: box })}
          onInsert={held ? undefined : (edgeId) => setShowing({ kind: "library", target: { edgeId } })}
        />
        {automation !== null && automation.placements.length === 0 && !held && (
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => setShowing({ kind: "library", target: { automationId: automation.id } })}
          >
            {t("auto.pic.first")}
          </button>
        )}
      </div>

      {automation !== null && showing?.kind === "library" && !held && (
        <Panel place={t("auto.pic.place")} title="" onClose={close}>
          <AutomationLibraryPanel
            // A new line pressed is a new pick: what was typed and opened for one line is not
            // carried to another.
            key={"edgeId" in showing.target ? `e${showing.target.edgeId}` : "first"}
            target={showing.target}
            projectId={projectId}
            where={whereTo(automation, showing.target)}
            onPlaced={close}
            onMake={() => setMaking(showing.target)}
          />
        </Panel>
      )}

      {automation !== null && showing?.kind === "about" && (
        <Panel place={t("auto.build.edit")} title={automation.name} onClose={close} readOnly={held}>
          <AutomationAboutPanel automation={automation} onDeleted={onBack} />
        </Panel>
      )}

      {pressed !== null && (
        <Panel place={t("auto.build.step")} title={builtinWord(pressed.builtin, pressed.name)} onClose={close}>
          <AutomationStepPanel
            automation={automation}
            placementId={pressed.id}
            onRemoved={close}
            onOpenAction={onOpenAction}
            readOnly={held}
          />
        </Panel>
      )}

      {making !== null && (
        <AutomationActionMake
          into={making}
          projectId={projectId}
          onMade={(actionId) => {
            close();
            onOpenAction(actionId);
          }}
          onClose={() => setMaking(null)}
        />
      )}
    </div>
  );
}
