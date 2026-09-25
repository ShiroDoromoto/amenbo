// **The library, opened in the build screen's panel** — what the `+` on a line, and the press on an
// empty picture, put in front of the reader (`AMB-T-5360`).
//
// **It is a panel and not a dialog**, because picking an action is done while looking at where it
// goes: a dialog over the picture would hide the very line it is about to be put on. The one thing
// that does open a dialog is making an action on the spot (`./AutomationActionMake`), which is a
// different act — the press after it leaves this screen for the new action's own.
//
// **Two lists, by reach.** This project's actions and the device's, each under its own head, since
// which of the two an action sits in says whether it is this project's own way of doing something or
// one every project shares. Both are searched by the one box over them, by name and by note, the way
// the library tab searches (`./AutomationActionsTab`).
//
// **The built-ins come third, under their own head** (`AMB-D-964`): Amenbo's own actions, listed from
// the code's definition, since a built-in's library action is only written the first time one is
// placed. Picking one is named by its key, and its library action is written on the way
// (`placeAutomationBuiltin`, `insertAutomationBuiltin`). The head is left out while the build carries
// none.
//
// **A row opens in place, with the button that places it.** What a reader weighs before placing is
// what the action asks and how it leaves — its settings, its inputs, its ways out — and that is read
// right under the row pressed rather than at the foot of a list that may run long.
//
// **On a line, the pressed way out comes to point at the new placement** and the new placement goes
// on to where that way out used to (`insertAutomationAction`). On an empty picture the placement
// stands on its own (`placeAutomationAction`).
import { useState } from "react";
import {
  insertAutomationAction,
  insertAutomationBuiltin,
  placeAutomationAction,
  placeAutomationBuiltin,
  useAutomationAction,
  useAutomationActions,
  useAutomationBuiltins,
} from "../core/automations";
import { errText, t } from "../core/i18n";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { ERROR_EXIT } from "./automationLayout";
import { CFG_KINDS } from "./automationPanel";
import { kindLabel } from "./automationPortKinds";
import { BuiltinDecl } from "./AutomationBuiltinScreen";
import { ExitMark, ReachChip, usedCount, WhereMark, type WhereTo } from "./automationParts";
import { builtinShown } from "../core/builtinWords";
import type { AutomationActionCardDto, AutomationBuiltinDto } from "../bindings/bindings";

/** Where the picked action goes: onto a line, or onto a picture with no line yet. */
export type PlaceTarget = { edgeId: number } | { automationId: number };

/** What an opened row shows: the action's declaration, read, and the press that places it. */
function Picked({ id, onPlace }: { id: number; onPlace: () => void }) {
  const action = useAutomationAction(id);
  const none = <span className="actdecl__none">{t("auto.act.none")}</span>;
  return (
    <div className="autolib__picked">
      {action !== null && action.note.trim() !== "" && (
        <div className="autolib__note">{action.note}</div>
      )}
      {action !== null && (
        <div className="actdecl__rows">
          <span className="actdecl__key">{t("auto.step.cfg")}</span>
          <span className="actdecl__chips">
            {action.settings.length === 0
              ? none
              : action.settings.map((one) => (
                  <span key={one.name} className="actport actport--cfg">
                    {one.name}
                    <span className="actport__kind">
                      {CFG_KINDS.find((kind) => kind.id === one.kind)?.label() ?? one.kind}
                    </span>
                  </span>
                ))}
          </span>
          <span className="actdecl__key">{t("auto.step.inputs")}</span>
          <span className="actdecl__chips">
            {action.inputs.length === 0
              ? none
              : action.inputs.map((one) => (
                  <span key={one.name} className={`actport actport--${one.kind}`}>
                    {one.name}
                    <span className="actport__kind">
                      {kindLabel(one.kind)}
                      {one.required && `・${t("auto.step.required")}`}
                    </span>
                  </span>
                ))}
          </span>
          <span className="actdecl__key">{t("auto.step.exits")}</span>
          <span className="actdecl__chips">
            {action.exits
              .filter((one) => one.name !== ERROR_EXIT)
              .map((one) => (
                <ExitMark key={one.id} name={one.name} outputs={one.outputs.map((out) => out.name)} />
              ))}
          </span>
        </div>
      )}
      <div>
        <button type="button" className="btn btn--primary" onClick={onPlace}>
          {t("auto.pic.placeDo")}
        </button>
      </div>
    </div>
  );
}

export function AutomationLibraryPanel({
  target,
  projectId,
  where,
  onPlaced,
  onMake,
}: {
  target: PlaceTarget;
  /** Whose library is reached — this project's own and the device's. */
  projectId: number | null;
  /** Where the placement will go: after which way out of which box, or first of all. */
  where: WhereTo;
  /** The action is on the picture; the panel has nothing left to show. */
  onPlaced: () => void;
  /** Make an action here instead — the dialog, opened on this same target. */
  onMake: () => void;
}) {
  const actions = useAutomationActions(projectId);
  const builtins = useAutomationBuiltins();
  const [words, setWords] = useState("");
  // An action's id, or a built-in's key: the two lists never share a row.
  const [picked, setPicked] = useState<number | string | null>(null);
  const [refused, setRefused] = useState<string | null>(null);

  // Core refuses an action another project's library holds, and a line that went away underneath;
  // the sentence it writes is what the panel draws, over the list the reader picked from.
  const place = (actionId: number) => {
    setRefused(null);
    const write =
      "edgeId" in target
        ? insertAutomationAction(target.edgeId, actionId)
        : placeAutomationAction(target.automationId, actionId);
    void write.then(onPlaced, (e: unknown) => setRefused(errText(e)));
  };
  const placeBuiltin = (key: string) => {
    setRefused(null);
    const write =
      "edgeId" in target
        ? insertAutomationBuiltin(target.edgeId, key)
        : placeAutomationBuiltin(target.automationId, key);
    void write.then(onPlaced, (e: unknown) => setRefused(errText(e)));
  };

  const w = words.trim().toLowerCase();
  const rows = (global: boolean) => {
    const found = actions.filter(
      (one: AutomationActionCardDto) =>
        one.global === global && (w === "" || `${one.name} ${one.note}`.toLowerCase().includes(w)),
    );
    if (found.length === 0) return <div className="autolib__none">{t("auto.actions.noMatch")}</div>;
    return found.map((one) => (
      <div key={one.id}>
        <button
          type="button"
          className={picked === one.id ? "autolib__row autolib__row--on" : "autolib__row"}
          aria-expanded={picked === one.id}
          onClick={() => setPicked(picked === one.id ? null : one.id)}
        >
          <span className="autolib__name">{one.name}</span>
          <span className="autolib__meta">
            {usedCount(one.usedBy)}
          </span>
        </button>
        {picked === one.id && <Picked id={one.id} onPlace={() => place(one.id)} />}
      </div>
    ));
  };

  const builtinRows = () => {
    // Searched in the words the reader sees, which are the screen's language and not the store's.
    const found = builtins
      .map(builtinShown)
      .filter((one: AutomationBuiltinDto) => w === "" || `${one.name} ${one.does}`.toLowerCase().includes(w));
    if (found.length === 0) return <div className="autolib__none">{t("auto.actions.noMatch")}</div>;
    return found.map((one) => (
      <div key={one.key}>
        <button
          type="button"
          className={picked === one.key ? "autolib__row autolib__row--on" : "autolib__row"}
          aria-expanded={picked === one.key}
          onClick={() => setPicked(picked === one.key ? null : one.key)}
        >
          <span className="autolib__name">{one.name}</span>
          <span className="autolib__meta">
            {usedCount(one.usedBy)}
          </span>
        </button>
        {picked === one.key && (
          <div className="autolib__picked">
            <div className="autolib__note">{one.does}</div>
            <BuiltinDecl builtin={one} />
            <div>
              <button type="button" className="btn btn--primary" onClick={() => placeBuiltin(one.key)}>
                {t("auto.pic.placeDo")}
              </button>
            </div>
          </div>
        )}
      </div>
    ));
  };

  return (
    <>
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}
      <WhereMark where={where} />
      <input
        {...asTyped}
        type="search"
        aria-label={t("auto.actions.search")}
        placeholder={t("auto.actions.search")}
        value={words}
        onChange={(e) => setWords(e.target.value)}
      />
      {projectId !== null && (
        <div className="autolib__group">
          <div className="autolib__head">
            <ReachChip global={false} />
          </div>
          {rows(false)}
        </div>
      )}
      <div className="autolib__group">
        <div className="autolib__head">
          <ReachChip global />
        </div>
        {rows(true)}
      </div>
      {builtins.length > 0 && (
        <div className="autolib__group">
          <div className="autolib__head">
            <ReachChip global builtin />
          </div>
          {builtinRows()}
        </div>
      )}
      <div className="autolib__make">
        <div className="autostep__said">{t("auto.lib.makeWhat")}</div>
        <div>
          <button type="button" className="btn" onClick={onMake}>
            {t("auto.lib.make")}
          </button>
        </div>
      </div>
    </>
  );
}
