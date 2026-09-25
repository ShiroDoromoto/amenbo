// One library action, opened — the screen its steps are built in (`AMB-T-5315`).
//
// **It is the automation build screen one layer down** (`AMB-D-949`). There the boxes are the actions
// placed on an automation; here they are the steps inside one action, and one step is one terminal.
// The picture, the panel and the add dialog are the same three (`./AutomationPicture`,
// `./AutomationActionStepPanel`, `./AutomationStepAdd`), handed this picture instead of that one.
//
// **There is no start press.** What is started is an automation, and an action is what one places —
// so what this screen has in that spot is the action's own name, its reach, and what a rewrite here
// reaches: every automation that places it. What it is for is written in the panel "Edit" opens, the
// way the automation's notes are (`./AutomationAboutPanel`), and like them it reaches no launch
// (`AMB-D-952`).
//
// **A step is added here, or put in on a line.** The `+` on a line is the road that leaves nothing
// pointing at nothing — but a picture has no line until two boxes are joined, so the press above it
// adds a step on its own: the first one, which an empty action takes as the step a placement opens
// (`../core/automations`), and every later one a reader then says what leads to with the way out's
// own pulldown (`./AutomationActionStepPanel`).
//
// **The head is the action, and the picture is the rest** (`AMB-T-5369`, `AMB-T-5526`). The head
// carries the name, its reach and how far a rewrite carries, and the one press that opens the action
// itself in the panel — what it is for is read there and on the list, not repeated over the picture.
// The input and the output are frames over and under the picture of the steps, named "takes in" and
// "exit", so the screen reads top to bottom as what comes in, what is done with it and what goes out
// without a line saying so. Pressing "Edit", either frame or a step opens that one in the panel to
// the right of the picture rather than stacked under it: a picture that runs
// long would otherwise carry a low step's contents off the bottom of the window, and the press would
// show nothing. The panel stands in the shell's right-pane column, where the board's detail does
// (`../shell/paneSlot`), so it scrolls on its own and is as tall as the window lets it be.
//
// **A global action opened from a project is read, not written** (`AMB-D-954`). It is no one
// project's, so it is changed from the sidebar's entrance and nowhere else: here the panels still open
// to be read, with everything in them held shut, nothing adds a step, and the head carries the lock
// and the one press that goes to it there in place of "Edit". One place to change a thing is what keeps a reader from wondering which
// of two is the real one.
//
// **An action a run is going on is read, not written, for as long as the run goes** (`AMB-D-961`). Core
// refuses every rewrite of it while a run of an automation placing it is running or paused, so the
// screen holds itself shut the way it does for a global action, and names those runs over the picture
// with the way to each one's pane (`./AutomationHeldBy`).
//
// **A built-in's action is read as its definition** (`./AutomationBuiltinScreen`). Its rows are Amenbo's
// own and core refuses every edit of them (`AMB-D-964`), so a box on a picture opening one lands on the
// screen that draws what it does rather than on steps nobody can change.
//
// **Which step is pressed is the screen's, not the picture's**, for the automation screen's reason:
// the picture marks that box and the panel draws that step, so it is held where both can see it. A
// step that is deleted takes the panel's selection with it.
import { useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { AutomationActionDeclaresPanel } from "./AutomationActionDeclaresPanel";
import { AutomationActionStepPanel } from "./AutomationActionStepPanel";
import { AutomationBuiltinScreen } from "./AutomationBuiltinScreen";
import { AutomationHeldBy } from "./AutomationHeldBy";
import { LockMark, ReachChip, usedCount } from "./automationParts";
import { AutomationPicture } from "./AutomationPicture";
import { AutomationStepAdd, type AddTarget } from "./AutomationStepAdd";
import { editAutomationAction, editAutomationStep, useAutomationAction } from "../core/automations";
import type { WhereTo } from "./automationParts";
import { actionGraph } from "./automationLayout";
import { errText, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { usePaneSlot } from "../shell/paneSlot";
import { useDraft, type Run } from "./automationPanel";
import { Sec } from "./automationDeclParts";
import type { AutomationActionDetailDto, AutomationPlacedOnDto } from "../bindings/bindings";

/** Where a step put in on a line goes: after which way out of which step, and before which. */
function whereTo(action: AutomationActionDetailDto | null, target: AddTarget): WhereTo {
  if (!("edgeId" in target)) return null;
  const edge = action?.edges.find((one) => one.id === target.edgeId);
  const from = action?.steps.find((one) => one.id === edge?.fromId);
  const to = action?.steps.find((one) => one.id === edge?.toId);
  return { box: from?.name ?? "", exit: edge?.exitName, next: to?.name };
}

/**
 * **The automations this action is placed on, by name** — each one where a rewrite here lands, and
 * each a press that goes to its build screen. The names say what "used by two" would leave the reader
 * to go and find.
 */
function PlacedOn({
  placedOn,
  projectId,
  onGoTo,
}: {
  placedOn: AutomationPlacedOnDto[];
  /** The project whose screen this is — from there, only its own automations are gone to. `null` is
   *  the sidebar's, which goes to any. */
  projectId: number | null;
  onGoTo?: (project: number, automation: number) => void;
}) {
  if (placedOn.length === 0) return <span className="autostep__label">{t("auto.actions.unused")}</span>;
  return (
    <div className="actplaced">
      {placedOn.map((one) =>
        onGoTo === undefined || (projectId !== null && one.project !== projectId) ? (
          <span key={one.id} className="actplaced__one">
            {one.name}
          </span>
        ) : (
          <button
            key={one.id}
            type="button"
            className="actplaced__one actplaced__one--go"
            onClick={() => onGoTo(one.project, one.id)}
          >
            {one.name}
            <span aria-hidden="true"> ↗</span>
          </button>
        ),
      )}
    </div>
  );
}

/** The panel's head as the box its name is typed in. It writes when the caret leaves, as every field does. */
function TitleInput({
  title,
  label,
  readOnly,
  onRename,
}: {
  title: string;
  label: string;
  readOnly: boolean;
  onRename: (to: string) => void;
}) {
  const [name, setName] = useDraft(title);
  return (
    <input
      className="actpanel__titlein"
      aria-label={tf("auto.act.nameOf", { place: label })}
      value={name}
      readOnly={readOnly}
      onChange={(e) => setName(e.target.value)}
      onBlur={() => name.trim() !== "" && name !== title && onRename(name)}
    />
  );
}

/**
 * The panel to the right of the picture: a head that names what it shows, and a way to close. It is
 * drawn into the shell's right-pane column, and in place where there is no shell to lend one.
 * The automation's build screen opens the same one beside its own picture (`./AutomationBuildScreen`).
 */
export function Panel({
  place,
  title,
  onRename,
  onClose,
  readOnly = false,
  children,
}: {
  place: string;
  /** What the panel shows — a name, or the field that writes one where the name is changed here. */
  title: ReactNode;
  /** Write a new name for what the panel shows. Given, the head's title — which is then the name as
   *  text — is the box it is typed in, so the name is not asked for again as a field under it. */
  onRename?: (to: string) => void;
  onClose: () => void;
  /** Hold every field and press in the body shut — the head's close stays live. */
  readOnly?: boolean;
  children: ReactNode;
}) {
  const pane = usePaneSlot();
  const panel = (
    <aside className="actpanel">
      <div className="actpanel__head">
        <span className="actbuild__sec">{place}</span>
        {onRename === undefined ? (
          <span className="actpanel__title">{title}</span>
        ) : (
          <TitleInput
            key={String(title)}
            title={String(title)}
            label={place}
            readOnly={readOnly}
            onRename={onRename}
          />
        )}
        <button
          type="button"
          className="actpanel__close"
          aria-label={t("auto.act.close")}
          onClick={onClose}
        >
          <Icon name="close" />
        </button>
      </div>
      {/* A disabled fieldset shuts every control under it, the panels' own included, without
          each of them having to be told. */}
      <fieldset className="actpanel__body" disabled={readOnly}>{children}</fieldset>
    </aside>
  );
  if (pane === null) return panel;
  // The column is drawn on the render after the claim, so the first render has nowhere to go yet.
  return pane.slot && createPortal(panel, pane.slot);
}

export function AutomationActionBuildScreen({
  id,
  projectId,
  onBack,
  onGoToGlobal,
  onGoToRun,
  onGoToAutomation,
}: {
  id: number;
  /** Whose project this is — what the machine is asked about when a step picks an agent. `null` is
   *  the sidebar's entrance, where a global action is changed. */
  projectId: number | null;
  onBack: () => void;
  /** Go to a global action on the sidebar's entrance, where it is changed. */
  onGoToGlobal?: (id: number) => void;
  /** Go to the pane a run holding this action is drawn in. */
  onGoToRun?: (project: number, run: number) => void;
  /** Go to the build screen of an automation this action is placed on. Absent, the names are read
   *  and not pressed. */
  onGoToAutomation?: (project: number, automation: number) => void;
}) {
  const action = useAutomationAction(id);
  const elsewhere = projectId !== null && action?.global === true;
  const readOnly = elsewhere || (action?.heldBy.length ?? 0) > 0;
  // What the panel is showing: a pressed step, the action itself, its input or its output — or
  // nothing, until one is pressed. An action opens on the picture, and a place picked for the reader
  // would be one they did not choose.
  const [step, setStep] = useState<number | null>(null);
  const [part, setPart] = useState<"about" | "in" | "out" | null>(null);
  // Where the dialog that writes a step is about to put one, while it is open.
  const [adding, setAdding] = useState<AddTarget | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const [note, setNote] = useDraft(action?.note ?? "");

  const run: Run = (write) => {
    setRefused(null);
    return Promise.resolve(write)
      .then(() => true)
      .catch((e: unknown) => {
        setRefused(errText(e));
        return false;
      });
  };

  const pickBox = (box: number | null) => {
    setPart(null);
    setStep(box);
  };
  const pickPart = (one: "about" | "in" | "out") => {
    setStep(null);
    setPart(part === one ? null : one);
  };
  const pressed = action?.steps.find((one) => one.id === step) ?? null;
  const partPlace = {
    about: t("auto.act.aboutPlace"),
    in: t("auto.pic.actionIn"),
    out: t("auto.pic.actionOut"),
  };

  if (action?.builtin !== undefined) {
    return <AutomationBuiltinScreen builtinKey={action.builtin} onBack={onBack} />;
  }

  return (
    <div className="actbuild">
      <div className="actbuild__head">
        <button type="button" className="btn" onClick={onBack}>
          <Icon name="chevronLeft" /> {t("auto.build.back")}
        </button>
        <span className="actbuild__name">{action?.name ?? ""}</span>
        {action !== null && (
          <>
            <ReachChip global={action.global} />
            <span className="actdecl__used">{usedCount(action.usedBy)}</span>
            {elsewhere ? (
              <>
                <LockMark />
                {onGoToGlobal && (
                  <button type="button" className="btn actbuild__edit" onClick={() => onGoToGlobal(action.id)}>
                    {t("auto.act.openInSidebar")}
                  </button>
                )}
              </>
            ) : (
              // "Edit" whether or not a run holds it: held, the panel it opens is shut, which is where
              // a reader finds out — the head does not change its word for it.
              <button
                type="button"
                className={part === "about" ? "btn btn--on actbuild__edit" : "btn actbuild__edit"}
                aria-pressed={part === "about"}
                onClick={() => pickPart("about")}
              >
                {t("auto.act.edit")}
              </button>
            )}
          </>
        )}
      </div>

      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      {action !== null && <AutomationHeldBy runs={action.heldBy} withAutomation onGoToRun={onGoToRun} />}

      <div className="actbuild__canvashead">
        <span className="actbuild__sec">{t("auto.act.stepsPlace")}</span>
        {action !== null && action.steps.length > 0 && !readOnly && (
          <button
            type="button"
            className="btn"
            onClick={() => setAdding({ picture: "action", actionId: action.id })}
          >
            {t("auto.act.stepAdd")}
          </button>
        )}
      </div>
      <div className="actbuild__canvas">
        <AutomationPicture
          graph={actionGraph(action)}
          insertLabel={t("auto.act.insert")}
          selectedBoxId={step ?? undefined}
          onPickBox={pickBox}
          onInsert={readOnly ? undefined : (edgeId) => setAdding({ picture: "action", edgeId })}
          selectedPart={part === "in" || part === "out" ? part : undefined}
          onPickPart={pickPart}
        />
        {action !== null && action.steps.length === 0 && !readOnly && (
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => setAdding({ picture: "action", actionId: action.id })}
          >
            {t("auto.act.firstStep")}
          </button>
        )}
      </div>

      {action !== null && part !== null && (
        <Panel
          place={partPlace[part]}
          title={action.name}
          onRename={
            part === "about" ? (to) => void run(editAutomationAction(action.id, { name: to })) : undefined
          }
          onClose={() => setPart(null)}
          readOnly={readOnly}
        >
          {part === "about" ? (
            <>
              <Sec title={t("auto.actions.note")}>
                <textarea
                  {...asTyped}
                  rows={3}
                  aria-label={t("auto.actions.note")}
                  placeholder={t("auto.actions.notePlaceholder")}
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  onBlur={() =>
                    note !== action.note && void run(editAutomationAction(action.id, { note }))
                  }
                />
              </Sec>
              <div className="autostep__pair">
                <span className="autostep__label">{t("auto.actions.reach")}</span>
                <span>
                  <ReachChip global={action.global} />
                </span>
              </div>
              <Sec title={t("auto.act.placedOn")}>
                <PlacedOn placedOn={action.placedOn} projectId={projectId} onGoTo={onGoToAutomation} />
              </Sec>
            </>
          ) : (
            <AutomationActionDeclaresPanel action={action} part={part} run={run} />
          )}
        </Panel>
      )}

      {pressed !== null && part === null && (
        <Panel
          place={t("auto.act.step")}
          title={pressed.name}
          onRename={(to) => void run(editAutomationStep(pressed.id, { name: to }))}
          onClose={() => setStep(null)}
          readOnly={readOnly}
        >
          <AutomationActionStepPanel
            action={action}
            stepId={step}
            onRemoved={() => setStep(null)}
          />
        </Panel>
      )}

      {adding !== null && (
        <AutomationStepAdd
          into={adding}
          where={whereTo(action, adding)}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
