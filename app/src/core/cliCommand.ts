// The name of the CLI beside this build, for every screen that hands over a command to type.
//
// There is more than one build a person may have open — production, the shared dev app, a task's own
// throwaway instance (`AMB-D-390`) — and each installs its CLI under its own name. A window that spells
// every command `amenbo` is naming a binary that is not necessarily there: from a dev window the reader
// types it and reaches production, or nothing at all.
//
// Only commands go through here. Prose that *mentions* the CLI ("the same check as `amenbo doctor`")
// stays as it is: it is talking about the product, not telling anyone what to type.
import { useEffect, useState } from "react";
import { fetchCliCommandName } from "./mutations";

/** The name a shipped build installs, and the fallback whenever there is no build to ask. */
export const PRODUCTION_CLI = "amenbo";

/**
 * The command this build installs, or `null` where it installs none a reader can run. Asked once per
 * screen that needs it — the channel is fixed at build time, so the answer cannot change while the
 * process runs — and it stands at the production name until the answer comes, so the shipped build,
 * where the answer *is* that name, never shows the change.
 *
 * `null` is the Linux preview, whose CLI is sealed inside an AppImage (`AMB-D-732`). A screen that
 * gets it stops wording a command and says there is none, which is the whole reason this is nullable:
 * a name that is not found is a reader following our instructions into a dead end.
 */
export function useCliCommandName(): string | null {
  const [cmd, setCmd] = useState<string | null>(PRODUCTION_CLI);
  useEffect(() => {
    let alive = true;
    fetchCliCommandName()
      .then((c) => alive && setCmd(c))
      .catch(() => {}); // Unanswered: the production name is still the likeliest one to be there.
    return () => {
      alive = false;
    };
  }, []);
  return cmd;
}
