// What the reader pressed went through, said where they pressed it (`AMB-D-686`).
//
// **The other half of `ErrorNote`.** A move that can fail says so in one shape; a move that worked
// said so in as many shapes as there were places to say it — a tick typed into the message string, a
// colour set inline beside it, and nothing marking the line as one worth reading aloud. Holding the
// three in one component is what keeps them together, and it is what makes the tick one drawing
// rather than a character each screen types for itself.
//
// **It takes the size of the words it stands in.** Where a screen says this in fine print, it is fine
// print; where it says it at the size of the row, it is that. The mark follows, `.icon` being sized
// from the tokens and the note setting none of its own (`AMB-D-687`).
//
// **A span, not a block.** It goes wherever the sentence it replaces went, which is sometimes inside
// one that is already running.
import type { ReactNode } from "react";
import { Icon } from "./Icon";

export function DoneNote({ children }: { children: ReactNode }) {
  return (
    <span className="donetext" role="status">
      <Icon name="check" />
      <span>{children}</span>
    </span>
  );
}
