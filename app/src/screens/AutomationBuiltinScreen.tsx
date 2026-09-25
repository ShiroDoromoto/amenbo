// One built-in, opened — read, never built (`AMB-D-964`).
//
// **A built-in is Amenbo's own.** What it does, the settings it reads, what it takes and the ways out
// it leaves by are written in the code that carries it out, so there is nothing here to change: the
// screen draws the definition under a lock. It is still opened, because a reader placing something
// they cannot see inside would not know what the run will do there.
//
// **It reads the definition, not an action.** A built-in's library action is only written the first
// time one is placed, so the one list every entrance has is the code's (`useAutomationBuiltins`) —
// and an action build screen opened on a built-in's action comes here too
// (`./AutomationActionBuildScreen`), since its rows are the same definition written out.
//
// **The declaration is written out in full here** (`BuiltinDecl`): its settings as well as what it
// receives and its ways out. The library panel shows only the last two under a picked row, the same
// way for a built-in as for an action of one's own (`./AutomationLibraryPanel`).
//
// **The lock says it, not a sentence** (`AMB-T-5525`): the reach chip and the lock beside it are
// what tell a reader nothing here is theirs to change, and a screen with no field in it bears that
// out. The way back is "back", because this screen is opened from the list and from an action's build
// screen alike, and a label naming one of them would be wrong for the other.
import { useAutomationBuiltins } from "../core/automations";
import { builtinShown } from "../core/builtinWords";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";
import { ExitMark, LockMark, ReachChip, usedCount } from "./automationParts";
import { CFG_KINDS } from "./automationPanel";
import { kindLabel } from "./automationPortKinds";
import type { AutomationBuiltinDto } from "../bindings/bindings";

/**
 * What a built-in reads, takes and leaves by — the three rows a reader weighs before placing one. It
 * draws the words it is handed, so it is handed them in the screen's language (`builtinShown`).
 *
 * **A row with nothing in it is a dash, on every row alike** — the ways out included, so all three
 * rows keep one shape whether or not the definition fills them.
 */
export function BuiltinDecl({ builtin }: { builtin: AutomationBuiltinDto }) {
  const none = <span className="actdecl__none">—</span>;
  return (
    <div className="actdecl__rows">
      <span className="actdecl__key">{t("auto.step.cfg")}</span>
      <span className="actdecl__chips">
        {builtin.settings.length === 0
          ? none
          : builtin.settings.map((one) => (
              <span key={one.name} className="actport actport--cfg">
                {one.name}
                <span className="actport__kind">
                  {CFG_KINDS.find((kind) => kind.id === one.kind)?.label() ?? one.kind}
                  {one.required && `・${t("auto.step.required")}`}
                </span>
              </span>
            ))}
      </span>
      <span className="actdecl__key">{t("auto.step.inputs")}</span>
      <span className="actdecl__chips">
        {builtin.inputs.length === 0
          ? none
          : builtin.inputs.map((one) => (
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
        {builtin.exits.length === 0
          ? none
          : builtin.exits.map((one) => (
              <ExitMark key={one.name} name={one.name} outputs={one.outputs.map((out) => out.name)} />
            ))}
      </span>
    </div>
  );
}

export function AutomationBuiltinScreen({
  builtinKey,
  onBack,
}: {
  /** Which built-in, by the key its definition carries. */
  builtinKey: string;
  onBack: () => void;
}) {
  const found = useAutomationBuiltins().find((one) => one.key === builtinKey);
  const builtin = found === undefined ? null : builtinShown(found);
  return (
    <div className="actbuild">
      <div className="actbuild__head">
        <button type="button" className="btn" onClick={onBack}>
          <Icon name="chevronLeft" /> {t("auto.builtin.back")}
        </button>
        <span className="actbuild__name">{builtin?.name ?? ""}</span>
        {builtin !== null && (
          <>
            <ReachChip global builtin />
            {/* Nowhere to change it: Amenbo carries it out as its code says. */}
            <LockMark />
            <span className="actdecl__used">{usedCount(builtin.usedBy)}</span>
          </>
        )}
      </div>
      {builtin !== null && (
        <div className="actdecl">
          <p className="actdecl__does">{builtin.does}</p>
          <BuiltinDecl builtin={builtin} />
        </div>
      )}
    </div>
  );
}
