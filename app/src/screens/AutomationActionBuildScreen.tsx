// One library action, opened — the screen its steps are built in (`AMB-T-5315`).
//
// **It is the automation build screen one layer down** (`AMB-D-949`). There the boxes are the actions
// placed on an automation; here they are the steps inside one action, and one step is one terminal.
// The picture, the panel and the add dialog are the same three (`./AutomationPicture`,
// `./AutomationActionStepPanel`, `./AutomationStepAdd`), handed this picture instead of that one.
//
// **There is no launch place.** What is started is an automation, and an action is what one places —
// so what this screen has in that spot is the action's own name, what it is for, its reach, and what
// a rewrite here reaches: every automation that places it. What it is for is written the way the
// automation's notes are (`./AutomationAboutPanel`), and like them it reaches no launch (`AMB-D-952`).
//
// **A step is added here, or put in on a line.** The `+` on a line is the road that leaves nothing
// pointing at nothing — but a picture has no line until two boxes are joined, so the press above it
// adds a step on its own: the first one, which an empty action takes as the step a placement opens
// (`../core/automations`), and every later one a reader then says what leads to with the way out's
// own pulldown (`./AutomationActionStepPanel`).
//
// **Four places, top to bottom: the action, its input, its steps and its output** (`AMB-T-5369`). The
// action is one row — what it is for, its reach and how far a rewrite carries — and the input and the
// output are frames over and under the picture of the steps, so the screen reads as what comes in,
// what is done with it and what goes out. Pressing the row's edit button, either frame or a step opens
// that one in the panel to the right of the picture rather than stacked under it: a picture that runs
// long would otherwise carry a low step's contents off the bottom of the window, and the press would
// show nothing. The panel stands in the shell's right-pane column, where the board's detail does
// (`../shell/paneSlot`), so it scrolls on its own and is as tall as the window lets it be.
//
// **A global action opened from a project is read, not written** (`AMB-D-954`). It is no one
// project's, so it is changed from the sidebar's entrance and nowhere else: here the panels still open
// to be read, with everything in them held shut, nothing adds a step, and the action's row carries the
// press that goes to it there. One place to change a thing is what keeps a reader from wondering which
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
import { editAutomationAction, useAutomationAction } from "../core/automations";
import { actionGraph } from "./automationLayout";
import { errText, t } from "../core/i18n";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { usePaneSlot } from "../shell/paneSlot";
import { useDraft, type Run } from "./automationPanel";
import type { AutomationActionDetailDto } from "../bindings/bindings";

/** The first line of what the action is for — all the band has room for; the panel holds the rest. */
function firstLine(note: string): string {
  return note.split("\n").find((line) => line.trim() !== "")?.trim() ?? "";
}

/**
 * **The action, in one row**: what it is for, its reach, and how many automations a rewrite here
 * reaches — read, not written. Its edit button opens the same fields in the panel.
 */
function AboutRow({
  action,
  editing,
  onEdit,
  readOnly,
  elsewhere,
  onGoToOwner,
}: {
  action: AutomationActionDetailDto;
  editing: boolean;
  onEdit: () => void;
  /** Whether it is read rather than written here — changed elsewhere, or held by a run. */
  readOnly: boolean;
  /** Whether it is changed elsewhere — a global action opened from a project. */
  elsewhere: boolean;
  /** Go to where it is changed — the sidebar's entrance, for a global action. */
  onGoToOwner?: () => void;
}) {
  return (
    <div className="actdecl">
      <div className="actdecl__head">
        <span className="actbuild__sec">{t("auto.act.aboutPlace")}</span>
        <span className="actdecl__note">
          {firstLine(action.note) !== "" ? (
            firstLine(action.note)
          ) : (
            <span className="actdecl__none">{t("auto.act.noNote")}</span>
          )}
        </span>
        <ReachChip global={action.global} />
        <span className="actdecl__used">{usedCount(action.usedBy)}</span>
        {elsewhere && <LockMark />}
        <button
          type="button"
          className={editing ? "btn btn--on" : "btn"}
          aria-pressed={editing}
          onClick={onEdit}
        >
          {readOnly ? t("auto.act.read") : t("auto.act.edit")}
        </button>
        {elsewhere && onGoToOwner && (
          <button type="button" className="btn" onClick={onGoToOwner}>
            {t("auto.act.openInSidebar")}
          </button>
        )}
      </div>
    </div>
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
  onClose,
  readOnly = false,
  children,
}: {
  place: string;
  title: string;
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
        <span className="actpanel__title">{title}</span>
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
  const [name, setName] = useDraft(action?.name ?? "");
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
      </div>

      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      {action !== null && (
        <AboutRow
          action={action}
          editing={part === "about"}
          onEdit={() => pickPart("about")}
          readOnly={readOnly}
          elsewhere={elsewhere}
          onGoToOwner={onGoToGlobal && (() => onGoToGlobal(action.id))}
        />
      )}

      {action !== null && <AutomationHeldBy runs={action.heldBy} onGoToRun={onGoToRun} />}

      <div className="actbuild__canvashead">
        <span className="actbuild__sec">{t("auto.act.stepsPlace")}</span>
        <span className="actbuild__hint">{t("auto.act.stepsHint")}</span>
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
      {action !== null && action.entryStepId === undefined && action.steps.length > 0 && (
        <div className="auto__notready actbuild__notready">{t("auto.act.noEntry")}</div>
      )}
      <div className="actbuild__canvas">
        <AutomationPicture
          graph={actionGraph(action)}
          empty={t("auto.act.empty")}
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
        <Panel place={partPlace[part]} title={action.name} onClose={() => setPart(null)} readOnly={readOnly}>
          {part === "about" ? (
            <>
              <label className="autostep__field">
                <span className="autostep__label">{t("auto.actions.name")}</span>
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  onBlur={() =>
                    name !== action.name && void run(editAutomationAction(action.id, { name }))
                  }
                />
              </label>
              <label className="autostep__field">
                <span className="autostep__label">{t("auto.actions.note")}</span>
                <textarea
                  {...asTyped}
                  rows={3}
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  onBlur={() =>
                    note !== action.note && void run(editAutomationAction(action.id, { note }))
                  }
                />
                <span className="autostep__said">{t("auto.actions.noteWhat")}</span>
              </label>
              <div className="autostep__field">
                <span className="autostep__label">{t("auto.actions.reach")}</span>
                <span>
                  <ReachChip global={action.global} />
                </span>
              </div>
              <div className="autostep__field">
                <span className="autostep__label">{t("auto.actions.colUsed")}</span>
                <span className="autostep__said">{usedCount(action.usedBy)}</span>
              </div>
            </>
          ) : (
            <AutomationActionDeclaresPanel action={action} part={part} run={run} />
          )}
        </Panel>
      )}

      {pressed !== null && part === null && (
        <Panel place={t("auto.act.step")} title={pressed.name} onClose={() => setStep(null)} readOnly={readOnly}>
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
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
