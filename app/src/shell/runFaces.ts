// Which face a run's pane is turned to — its roles or its terminal (`./RunPicture`).
//
// **The face is kept by run, for this window.** A pane comes down when it is carried onto another page
// or its project is left, and goes up again for the same run, which is the pane still, and should come
// back as the reader left it. A run not in here opens on its roles — every run does, the first time —
// and nothing is kept for the app as a whole: one reader turning one pane to its terminal says nothing
// about the next run.
//
// **A press elsewhere can turn a run's pane to its terminal**: its seat pressed among the roles of its
// launch's pane, which puts that seat in front, or the task a failed round was filed as. The pane may
// be up already, so it is told as well as written down; one not up yet reads it here as it goes up.
//
// **A new round can turn it back to its roles**: a launch's pane whose seat in front is over goes back
// to the roles as another round of the launch starts (`./WorkspaceFace`), where that round is seen.

/** What a run's pane shows under its row: its roles, or the terminal (`./RunPicture`). */
export type Face = "roles" | "terminal";

const faces = new Map<number, Face>();
const heard = new Set<(run: number, face: Face) => void>();

/** The face the reader last turned the run `run`'s pane to, where they have. */
export function faceOf(run: number): Face | undefined {
  return faces.get(run);
}

/** Keep the face `face` as the reader's turn for the run `run`'s pane. */
export function keepFace(run: number, face: Face): void {
  faces.set(run, face);
}

/** Turn the run `run`'s pane to `face`: kept as its face, and told to a pane that is up. */
function turnTo(run: number, face: Face): void {
  faces.set(run, face);
  for (const one of heard) one(run, face);
}

/** Turn the run `run`'s pane to its terminal (`turnTo`). */
export function showTerminalOf(run: number): void {
  turnTo(run, "terminal");
}

/** Turn the run `run`'s pane back to its roles (`turnTo`). */
export function showRolesOf(run: number): void {
  turnTo(run, "roles");
}

/** Hear every run turned by {@link showTerminalOf} or {@link showRolesOf}, until the answer is called. */
export function onFaceTurned(hear: (run: number, face: Face) => void): () => void {
  heard.add(hear);
  return () => {
    heard.delete(hear);
  };
}
