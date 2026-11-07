// Which face a run's pane is turned to — its roles or its terminal (`./RunPicture`).
//
// **The face is kept by run, for this window.** A pane comes down when it is carried onto another page
// or its project is left, and goes up again for the same run, which is the pane still, and should come
// back as the reader left it. A run not in here opens on its roles — every run does, the first time —
// and nothing is kept for the app as a whole: one reader turning one pane to its terminal says nothing
// about the next run.
//
// **A press elsewhere can turn a run's pane to its terminal**: a round on the bar over another run's
// terminal, or the task a failed round was filed as. The pane may be up already, so it is told as well
// as written down; one not up yet reads it here as it goes up.

/** What a run's pane shows under its row: its roles, or the terminal (`./RunPicture`). */
export type Face = "roles" | "terminal";

const faces = new Map<number, Face>();
const heard = new Set<(run: number) => void>();

/** The face the reader last turned the run `run`'s pane to, where they have. */
export function faceOf(run: number): Face | undefined {
  return faces.get(run);
}

/** Keep the face `face` as the reader's turn for the run `run`'s pane. */
export function keepFace(run: number, face: Face): void {
  faces.set(run, face);
}

/** Turn the run `run`'s pane to its terminal: kept as the reader's turn, and told to a pane that is up. */
export function showTerminalOf(run: number): void {
  faces.set(run, "terminal");
  for (const one of heard) one(run);
}

/** Hear every run turned to its terminal by {@link showTerminalOf}, until the answer is called. */
export function onTerminalShown(hear: (run: number) => void): () => void {
  heard.add(hear);
  return () => {
    heard.delete(hear);
  };
}
