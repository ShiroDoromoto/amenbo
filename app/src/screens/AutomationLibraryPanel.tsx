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
// what the action takes in and how it leaves, and that is read right under the row pressed rather
// than at the foot of a list that may run long. **An action of one's own and a built-in open the same
// way**: the same two rows, what it receives and its ways out, the error one among them with its own
// mark — so the two kinds read as one list and not as two screens.
//
// **A group with nothing matching the search is not drawn at all**, head and all: one "nothing
// matches" per group said the same thing three times. What stays at the foot whatever was typed is
// the row that makes a new action, under the name typed — the reader who searched and did not find
// it goes on from where they are, without a sentence telling them they may.
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
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { errText, t, tf } from "../core/i18n";
import { ERROR_EXIT } from "./automationLayout";
import { kindLabel } from "./automationPortKinds";
import { ExitMark, ReachChip, usedCount, WhereMark, type WhereTo } from "./automationParts";
import { builtinShown } from "../core/builtinWords";
import type {
  AutomationActionCardDto,
  AutomationBuiltinDto,
  AutomationPortDto,
} from "../bindings/bindings";

/** Where the picked action goes: onto a line, or onto a picture with no line yet. */
export type PlaceTarget = { edgeId: number } | { automationId: number };

/**
 * What an opened row declares, the same two rows for an action of one's own and a built-in: what it
 * receives, and its ways out. Every box is born with the error way out, so it is always drawn, last.
 */
function Declared({
  inputs,
  exits,
}: {
  inputs: readonly AutomationPortDto[];
  exits: readonly { name: string; outputs: readonly { name: string }[] }[];
}) {
  const named = exits.filter((one) => one.name !== ERROR_EXIT);
  return (
    <div className="actdecl__rows">
      <span className="actdecl__key">{t("auto.pic.actionIn")}</span>
      <span className="actdecl__chips">
        {inputs.length === 0 ? (
          <span className="actdecl__none">{t("auto.act.none")}</span>
        ) : (
          inputs.map((one) => (
            <span key={one.name} className={`actport actport--${one.kind}`}>
              {one.name}
              <span className="actport__kind">
                {kindLabel(one.kind)}
                {one.required && `・${t("auto.step.required")}`}
              </span>
            </span>
          ))
        )}
      </span>
      <span className="actdecl__key">{t("auto.step.exits")}</span>
      <span className="actdecl__chips">
        {named.map((one) => (
          <ExitMark
            key={one.name}
            name={one.name}
            outputs={one.outputs.map((out) => out.name)}
          />
        ))}
        <ExitMark name={ERROR_EXIT} />
      </span>
    </div>
  );
}

/** What an opened row shows: the action's note and declaration, and the press that places it. */
function Picked({ id, onPlace }: { id: number; onPlace: () => void }) {
  const action = useAutomationAction(id);
  return (
    <div className="autolib__picked">
      {action !== null && action.note.trim() !== "" && (
        <div className="autolib__note">{action.note}</div>
      )}
      {action !== null && <Declared inputs={action.inputs} exits={action.exits} />}
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
  /** Make an action here instead — the dialog, opened on this same target with the name typed. */
  onMake: (name: string) => void;
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
  const own = (global: boolean) =>
    actions.filter(
      (one: AutomationActionCardDto) =>
        one.global === global && (w === "" || `${one.name} ${one.note}`.toLowerCase().includes(w)),
    );
  // Searched in the words the reader sees, which are the screen's language and not the store's.
  const shownBuiltins = builtins
    .map(builtinShown)
    .filter((one: AutomationBuiltinDto) => w === "" || `${one.name} ${one.does}`.toLowerCase().includes(w));

  const rows = (found: AutomationActionCardDto[]) =>
    found.map((one) => (
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

  const builtinRows = () =>
    shownBuiltins.map((one) => (
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
            <Declared inputs={one.inputs} exits={one.exits} />
            <div>
              <button type="button" className="btn btn--primary" onClick={() => placeBuiltin(one.key)}>
                {t("auto.pic.placeDo")}
              </button>
            </div>
          </div>
        )}
      </div>
    ));

  const mine = projectId === null ? [] : own(false);
  const shared = own(true);
  const typed = words.trim();

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
      {mine.length > 0 && (
        <div className="autolib__group">
          <div className="autolib__head">
            <ReachChip global={false} />
          </div>
          {rows(mine)}
        </div>
      )}
      {shared.length > 0 && (
        <div className="autolib__group">
          <div className="autolib__head">
            <ReachChip global />
          </div>
          {rows(shared)}
        </div>
      )}
      {shownBuiltins.length > 0 && (
        <div className="autolib__group">
          <div className="autolib__head">
            <ReachChip global builtin />
          </div>
          {builtinRows()}
        </div>
      )}
      <button type="button" className="autolib__make" onClick={() => onMake(typed)}>
        {typed === "" ? t("auto.lib.makeNew") : tf("auto.lib.makeNamed", { name: typed })}
      </button>
    </>
  );
}
