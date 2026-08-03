// The folders a project is bound to, read once for whoever on the screen needs them.
//
// A project is bound to its folders and not the other way round, so anything that speaks about "this
// project's folder" has to ask. Three things on the board do: whether to warn that there is none
// (`AMB-D-533`), which folder the first loop opens a terminal in, and — since the answer decides which
// standing notice is drawn at all — the board's own ordering.
//
// `answered` is the half that is easy to drop. Nothing should be drawn from an unanswered read: a flash
// of "no folder here" on a project that has one reads as a broken binding, which is worse to say than
// nothing. A read that fails is treated as answered-with-none, since the invitation to link one is the
// safe half of being wrong.
import { useEffect, useState } from "react";
import { fetchBoundFolders } from "./mutations";
import type { BoundFolderDto } from "../bindings/bindings";

export type BoundFolders = {
  /** Every folder recorded for this project, gone ones included. */
  all: BoundFolderDto[];
  /** The ones that are actually there — the only ones an AI can be started in. */
  live: BoundFolderDto[];
  /** False until the read comes back. Draw nothing before it. */
  answered: boolean;
};

export function useBoundFolders(projectId: number): BoundFolders {
  const [all, setAll] = useState<BoundFolderDto[]>([]);
  const [answered, setAnswered] = useState(false);

  useEffect(() => {
    let alive = true;
    setAll([]);
    setAnswered(false);
    fetchBoundFolders(projectId)
      .then((folders) => {
        if (!alive) return;
        setAll(folders);
        setAnswered(true);
      })
      .catch(() => { if (alive) setAnswered(true); });
    return () => { alive = false; };
  }, [projectId]);

  return { all, live: all.filter((one) => one.exists), answered };
}
