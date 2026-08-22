// What a screen says in place of a command, when this build ships none a reader can run.
//
// One build reaches this: a theme's development preview on Linux (`AMB-D-732`), which is a single
// AppImage whose contents are mounted for as long as the app is open and gone after. The CLI is in
// there, but it has no address that outlives the run, so there is nothing to name — and naming
// something anyway is worse than saying so, because a reader who follows it gets `not found` and no
// idea why.
//
// It is one line, deliberately: the reader on that build came for the GUI, and the CLI is not a step
// they are stuck on. Every screen that hands over a command shows the same sentence, so a member
// meeting it twice reads it as one fact about their build rather than two failures.
import { t } from "../core/i18n";

/** The sentence itself, for the screens that would otherwise be wording a command. */
export function NoCli() {
  return <p className="muted">{t("cli.none")}</p>;
}
