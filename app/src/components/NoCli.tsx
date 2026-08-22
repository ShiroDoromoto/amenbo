// What a screen says in place of a command, when nothing on this machine reaches this build's CLI.
//
// One build reaches this: a theme's development preview on Linux (`AMB-D-732`). It is a single
// AppImage whose contents are mounted for as long as the app is open and gone after, so the copy
// inside it has no address that outlives the run — which is why the preview ships the CLI beside it
// as its own file. Nothing installs that file but the member, so this is the state before they do,
// and the sentence says what to do rather than only what is missing. It stops showing on its own:
// `Paths::command_to_run` asks `PATH`, so the command appears the moment it is there.
//
// It is one line, deliberately: the reader on that build came for the GUI, and the CLI is not a step
// they are stuck on. Every screen that hands over a command shows the same sentence, so a member
// meeting it twice reads it as one fact about their build rather than two failures.
import { t } from "../core/i18n";

/** The sentence itself, for the screens that would otherwise be wording a command. */
export function NoCli() {
  return <p className="muted">{t("cli.none")}</p>;
}
