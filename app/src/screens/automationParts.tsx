// The small parts every automation screen draws the same way — the list, the build screens, their
// panels and the dialogs.
//
// **One meaning, one part, one word.** Where an action is kept, how many automations use it, that
// it cannot be changed here, a way out, and where a placement is about to go are each drawn once,
// here, because the same thing said three ways reads as three things. A screen hands a part the
// facts rather than writing its own words around them, so no screen needs a sentence explaining it.
import { t, tn } from "../core/i18n";
import { builtinWord } from "../core/builtinWords";
import { Icon } from "../components/Icon";
import { ERROR_EXIT } from "./automationLayout";
import { exitLabel } from "./automationPanel";

/**
 * The reach an action sits in, as a chip — the same chip wherever an action is named. A built-in is
 * kept on the device's shelf, but what it is to a reader is Amenbo's own.
 */
export function ReachChip({ global, builtin = false }: { global: boolean; builtin?: boolean }) {
  const tone = builtin ? "actscope actscope--builtin" : global ? "actscope actscope--global" : "actscope";
  return (
    <span className={tone}>
      <em aria-hidden="true" />
      {builtin
        ? t("auto.actions.reachBuiltin")
        : global
          ? t("auto.actions.reachGlobal")
          : t("auto.actions.reachProject")}
    </span>
  );
}

/** How many automations use an action, in the one way every screen says it. */
export function usedCount(n: number): string {
  return n === 0 ? t("auto.actions.unused") : tn("auto.actions.usedBy", n);
}

/**
 * That what is shown cannot be changed here, as a mark rather than a sentence. Where it can be
 * changed is the button beside it, which the screen draws because only the screen knows where that is.
 */
export function LockMark() {
  return (
    <span className="lockmark" title={t("auto.lockMark")}>
      <Icon name="lock" label={t("auto.lockMark")} />
    </span>
  );
}

/**
 * One way out, as a mark — the same one in the picture, a panel and a dialog. The error way out is
 * drawn dashed in the stop colour, so it reads apart from the ones a reader named. `builtin` is the
 * key of the built-in the name belongs to, which puts its words in the screen's language.
 */
export function ExitMark({
  name,
  builtin,
  outputs = [],
}: {
  name: string | undefined;
  builtin?: string | null;
  /** What the way out hands on, already in the words the screen shows. */
  outputs?: readonly string[];
}) {
  const error = name === ERROR_EXIT;
  return (
    <span className={error ? "actport actport--error" : "actport actport--exit"}>
      {exitLabel(name === undefined || error ? name : builtinWord(builtin, name))}
      {outputs.length > 0 && <span className="actport__kind">{outputs.join("・")}</span>}
    </span>
  );
}

/** Where a placement is about to go: after one way out of one box, or onto a picture with no line. */
export type WhereTo = { box: string; exit: string | undefined; builtin?: string | null } | null;

/**
 * Where a placement is about to go, as a small picture of the line it goes on: the box before it,
 * the way out it hangs on, and a dashed "here". Onto an empty picture there is nothing before it,
 * so the "here" stands alone.
 */
export function WhereMark({ where }: { where: WhereTo }) {
  return (
    <div className="wheremark">
      {where !== null && (
        <>
          <span className="wheremark__box">{where.box}</span>
          <span className="wheremark__line" aria-hidden="true">─</span>
          <ExitMark name={where.exit} builtin={where.builtin} />
          <span className="wheremark__line" aria-hidden="true">─▶</span>
        </>
      )}
      <span className="wheremark__here">{t("auto.lib.here")}</span>
    </div>
  );
}
