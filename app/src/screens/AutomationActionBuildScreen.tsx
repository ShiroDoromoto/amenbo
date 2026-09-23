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
// that one in the panel pinned to the right of the picture rather than stacked under it: a picture
// that runs long would otherwise carry a low step's contents off the bottom of the window, and the
// press would show nothing.
//
// **Which step is pressed is the screen's, not the picture's**, for the automation screen's reason:
// the picture marks that box and the panel draws that step, so it is held where both can see it. A
// step that is deleted takes the panel's selection with it.
import { useState, type ReactNode } from "react";
import { AutomationActionDeclaresPanel } from "./AutomationActionDeclaresPanel";
import { AutomationActionStepPanel } from "./AutomationActionStepPanel";
import { ReachChip } from "./AutomationActionsTab";
import { AutomationPicture } from "./AutomationPicture";
import { AutomationStepAdd, type AddTarget } from "./AutomationStepAdd";
import { editAutomationAction, useAutomationAction } from "../core/automations";
import { actionGraph } from "./automationLayout";
import { errText, t, tf, tn } from "../core/i18n";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { useDraft, type Run } from "./automationPanel";
import type { AutomationActionDetailDto } from "../bindings/bindings";

/**
 * What carries out a step put in on a line — the likeliest answer for the one being put in front of
 * it. An action whose steps name none falls back to the first agent the catalog lists, which is what
 * `automation step-add` asks for and never guesses.
 */
function agentOn(action: AutomationActionDetailDto | null, edgeId: number): string {
  const edge = action?.edges.find((one) => one.id === edgeId);
  return action?.steps.find((one) => one.id === edge?.fromId)?.agent ?? "claude-code";
}

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
}: {
  action: AutomationActionDetailDto;
  editing: boolean;
  onEdit: () => void;
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
        <span className="actdecl__used">
          {action.usedBy === 0 ? t("auto.actions.unused") : tn("auto.actions.usedBy", action.usedBy)}
        </span>
        <button
          type="button"
          className={editing ? "btn btn--on" : "btn"}
          aria-pressed={editing}
          onClick={onEdit}
        >
          {t("auto.act.edit")}
        </button>
      </div>
    </div>
  );
}

/**
 * The panel pinned to the right of the picture: a head that names what it shows, and a way to close.
 * The automation's build screen pins the same one beside its own picture (`./AutomationBuildScreen`).
 */
export function Panel({
  place,
  title,
  onClose,
  children,
}: {
  place: string;
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <aside className="actpanel">
      <div className="actpanel__inner">
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
        <div className="actpanel__body">{children}</div>
      </div>
    </aside>
  );
}

export function AutomationActionBuildScreen({
  id,
  projectId,
  onBack,
}: {
  id: number;
  /** Whose project this is — what the machine is asked about when a step picks an agent. */
  projectId: number | null;
  onBack: () => void;
}) {
  const action = useAutomationAction(id);
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
  const panelOpen = action !== null && (part !== null || pressed !== null);
  const partPlace = {
    about: t("auto.act.aboutPlace"),
    in: t("auto.pic.actionIn"),
    out: t("auto.pic.actionOut"),
  };

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
        <AboutRow action={action} editing={part === "about"} onEdit={() => pickPart("about")} />
      )}

      <div className={panelOpen ? "actbuild__stage actbuild__stage--panel" : "actbuild__stage"}>
        <div className="actbuild__canvashead">
          <span className="actbuild__sec">{t("auto.act.stepsPlace")}</span>
          <span className="actbuild__hint">{t("auto.act.stepsHint")}</span>
          {action !== null && action.steps.length > 0 && (
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
            onInsert={(edgeId) => setAdding({ picture: "action", edgeId })}
            selectedPart={part === "in" || part === "out" ? part : undefined}
            onPickPart={pickPart}
          />
          {action !== null && action.steps.length === 0 && (
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
          <Panel place={partPlace[part]} title={action.name} onClose={() => setPart(null)}>
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
                  <span className="autostep__said">
                    {action.usedBy === 0
                      ? t("auto.actions.usedNone")
                      : tf("auto.act.usedWhere", { n: action.usedBy })}
                  </span>
                </div>
              </>
            ) : (
              <AutomationActionDeclaresPanel action={action} part={part} run={run} />
            )}
          </Panel>
        )}

        {pressed !== null && part === null && (
          <Panel place={t("auto.act.step")} title={pressed.name} onClose={() => setStep(null)}>
            <AutomationActionStepPanel
              action={action}
              stepId={step}
              projectId={projectId}
              onRemoved={() => setStep(null)}
            />
          </Panel>
        )}
      </div>

      {adding !== null && (
        <AutomationStepAdd
          into={adding}
          projectId={projectId}
          agent={"edgeId" in adding ? agentOn(action, adding.edgeId) : "claude-code"}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
