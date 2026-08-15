// What went wrong, said where the reader was working (`AMB-D-686`).
//
// **One shape, drawn once.** A note is three things at once — the mark, the stop step, and
// `role="alert"` so a reader who is not looking at that corner is told anyway — and they are worth
// nothing apart: a note that keeps the look and loses the role is silent to the reader who most needs
// it, and nothing on screen says so. Holding them in one component is what keeps them together, and it
// is also what makes the mark one drawing rather than a character each screen types for itself.
//
// **The tone is what varies, and it is the only thing that does.** A form refusing a value is loud,
// a settings row that could not save is not — and neither is a different arrangement of the same
// parts, so it is one prop rather than a second component.
import type { ReactNode } from "react";
import { Icon } from "./Icon";

/**
 * `quiet` is for a note standing in a row of settings, where the failure is one line among the rows
 * rather than the answer to something the reader just pressed. It takes the size of the text around
 * it; the loud one sets its own.
 */
export function ErrorNote({ children, tone = "danger" }: { children: ReactNode; tone?: "danger" | "quiet" }) {
  return (
    <div className={tone === "quiet" ? "errortext errortext--quiet" : "errortext"} role="alert">
      <Icon name="warning" />
      <span>{children}</span>
    </div>
  );
}
