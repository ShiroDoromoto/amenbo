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
// **The declaration is the one the library panel shows under a picked row** (`BuiltinDecl`), so a
// reader sees the same three rows wherever they weigh one.
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
 */
export function BuiltinDecl({ builtin }: { builtin: AutomationBuiltinDto }) {
  const none = <span className="actdecl__none">{t("auto.act.none")}</span>;
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
        {builtin.exits.map((one) => (
          <ExitMark key={one.name ?? ""} name={one.name ?? undefined} outputs={one.outputs.map((out) => out.name)} />
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
          <Icon name="chevronLeft" /> {t("auto.build.back")}
        </button>
        <span className="actbuild__name">{builtin?.name ?? ""}</span>
      </div>
      {builtin !== null && (
        <div className="actdecl">
          <div className="actdecl__head">
            <span className="actbuild__sec">{t("auto.builtin.does")}</span>
            <span className="actdecl__note">{builtin.does}</span>
            <ReachChip global builtin />
            <span className="actdecl__used">{usedCount(builtin.usedBy)}</span>
            {/* Nowhere to change it: Amenbo carries it out as its code says. */}
            <LockMark />
          </div>
          <BuiltinDecl builtin={builtin} />
        </div>
      )}
    </div>
  );
}
