// **A number between two bounds, stepped one at a time or typed**: "−", the number, "+". Used for how
// many a role has, on the build screen; the run's pane sets the same number for one launch alone, in
// the role's menu, drawn smaller (`small`).
//
// **The number is a box to type in.** Stepping a long way one press at a time is slow, so the number
// itself takes a typed one. A typed number reaches `onStep` only when it is a whole number within the
// bounds (full-width digits read as digits); anything else — a number out of the bounds, a word, an
// empty box — is refused on the spot: the box is marked invalid and the range is said under it, and
// nothing is handed on. When the number is written is the caller's to say; the stepper only hands it.
//
// **What is typed stays while the caret is in the box.** A number the caller hands back while the reader
// is still typing — the echo of a write, or of the digit before — does not overwrite the box; once the
// caret leaves, the box shows the caller's number again as soon as the caller's number moves. A refused
// number stays in the box, marked, so the reader sees what they typed. A press of "−" or "+" steps from
// the caller's number and drops what was typed. The range said under a refused number is laid over what
// lies below the stepper, so the stepper does not move while the reader types.
//
// **A caller that writes only on a press of its own learns of a refused number** (`onRefused`): told
// each time the box turns refused or turns back, it can keep its press from writing the number last
// handed on while the box shows another.
//
// **A number that acts on a launch at once is stepped only** (`typable={false}`): each digit typed would
// reach the launch on its way to the number meant, so there the number is drawn, not a box.
//
// **The press keeps the focus.** A button pressed with the pointer is not focused in every web view
// (WebKit leaves it), so the press gives it the focus itself: a reader stepping with the keyboard or
// the mouse goes on from the button they pressed. A button that just reached its bound is disabled,
// and the focus is let go with it.
//
// **It is one field to the screen's "Written" mark** (`data-saved-field`, `../screens/AutomationSaved`):
// the mark stands beside the whole stepper, never over the number just stepped to or typed.
import { useEffect, useId, useState, type ChangeEvent, type MouseEvent } from "react";
import { tf } from "../core/i18n";

/** The whole number a typed text stands for, or null where it is not one. */
export function typedNumber(text: string): number | null {
  const plain = text.normalize("NFKC").trim();
  return /^\d+$/.test(plain) ? Number(plain) : null;
}

export function Stepper({
  value,
  min,
  max,
  label,
  valueTemplate,
  lessLabel,
  moreLabel,
  onStep,
  small = false,
  typable = true,
  onRefused,
}: {
  value: number;
  min: number;
  max: number;
  /** What the group is the number of, read out for the whole of it. */
  label: string;
  /** How the number is drawn with its unit, `{n}` standing for the number (`{n} items`). */
  valueTemplate: string;
  lessLabel: string;
  moreLabel: string;
  onStep: (next: number) => void;
  /** Drawn smaller, for a stepper inside a menu. */
  small?: boolean;
  /** Takes a typed number; false draws the number and leaves only "−" and "+". */
  typable?: boolean;
  /** Told whether the box now holds a refused number, each time that turns. */
  onRefused?: (refused: boolean) => void;
}) {
  const rangeId = useId();
  // What the reader has typed, while it differs from the caller's number; null shows the caller's.
  const [typed, setTyped] = useState<string | null>(null);
  const [typing, setTyping] = useState(false);
  const [shown, setShown] = useState(value);
  if (value !== shown) {
    setShown(value);
    if (!typing) setTyped(null);
  }

  const text = typed ?? String(value);
  const n = typedNumber(text);
  const refused = typable && (n === null || n < min || n > max);
  const [before, after = ""] = valueTemplate.split("{n}");
  useEffect(() => {
    onRefused?.(refused);
    // Told on a turn only: a caller handing a new function each draw is not told again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refused]);

  const step = (by: number) => (e: MouseEvent<HTMLButtonElement>) => {
    e.currentTarget.focus();
    setTyped(null);
    onStep(value + by);
  };
  const type = (e: ChangeEvent<HTMLInputElement>) => {
    const next = e.currentTarget.value;
    setTyped(next);
    const m = typedNumber(next);
    if (m !== null && m >= min && m <= max && m !== value) onStep(m);
  };
  const leave = () => {
    setTyping(false);
    if (typed !== null && typedNumber(typed) === value) setTyped(null);
  };

  return (
    <span className="stepfield">
      <span className={small ? "stepper stepper--s" : "stepper"} role="group" aria-label={label} data-saved-field="">
        <button type="button" aria-label={lessLabel} disabled={value <= min} onClick={step(-1)}>
          −
        </button>
        {!typable ? (
          <output className="stepper__value">{before + value + after}</output>
        ) : (
          <label className="stepper__value">
            {before}
            <input
              type="text"
              inputMode="numeric"
              autoComplete="off"
              className="stepper__input"
              aria-label={label}
              aria-invalid={refused || undefined}
              aria-describedby={refused ? rangeId : undefined}
              value={text}
              onFocus={() => setTyping(true)}
              onBlur={leave}
              onChange={type}
            />
            {after}
          </label>
        )}
        <button type="button" aria-label={moreLabel} disabled={value >= max} onClick={step(1)}>
          ＋
        </button>
      </span>
      {refused && (
        <span id={rangeId} className="stepfield__range" role="alert">
          {tf("auto.side.stepperRange", { min, max })}
        </span>
      )}
    </span>
  );
}
