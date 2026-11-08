// **A number stepped one at a time between two bounds**: "−", the number, "+". Used for how many a
// role has, on the build screen; the run's pane steps the same number for one launch alone, in the
// role's menu, drawn smaller (`small`).
//
// **The press keeps the focus.** A button pressed with the pointer is not focused in every web view
// (WebKit leaves it), so the press gives it the focus itself: a reader stepping with the keyboard or
// the mouse goes on from the button they pressed. A button that just reached its bound is disabled,
// and the focus is let go with it.
//
// **It is one field to the screen's "Written" mark** (`data-saved-field`, `../screens/AutomationSaved`):
// the mark stands beside the whole stepper, never over the number just stepped to.
import type { MouseEvent } from "react";

export function Stepper({
  value,
  min,
  max,
  label,
  valueText,
  lessLabel,
  moreLabel,
  onStep,
  small = false,
}: {
  value: number;
  min: number;
  max: number;
  /** What the group is the number of, read out for the whole of it. */
  label: string;
  /** The number as it is drawn, with its unit. */
  valueText: string;
  lessLabel: string;
  moreLabel: string;
  onStep: (next: number) => void;
  /** Drawn smaller, for a stepper inside a menu. */
  small?: boolean;
}) {
  const step = (by: number) => (e: MouseEvent<HTMLButtonElement>) => {
    e.currentTarget.focus();
    onStep(value + by);
  };
  return (
    <span className={small ? "stepper stepper--s" : "stepper"} role="group" aria-label={label} data-saved-field="">
      <button type="button" aria-label={lessLabel} disabled={value <= min} onClick={step(-1)}>
        −
      </button>
      <output className="stepper__value">{valueText}</output>
      <button type="button" aria-label={moreLabel} disabled={value >= max} onClick={step(1)}>
        ＋
      </button>
    </span>
  );
}
