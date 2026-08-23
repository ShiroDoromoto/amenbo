// Whether the terminal segment is wearing a badge — the one knock a person gets while the terminal
// is not the face they are looking at (`AMB-D-753`).
//
// In one window the two faces take turns, so a pane that has asked for a person's turn is behind the
// face that is up: the nameplate saying so, and the dot beside it, are both on the other side of the
// switch. The badge is what crosses it, and it is deliberately the *only* thing that does — the
// segment says a turn is standing, and nothing about whose or what it is.
//
// **Output is not a turn.** The badge follows `waiting` alone (`../talk/sessions`), which an agent
// says of itself; a build streaming past does not raise it, and would raise it continuously if it
// did. That is the whole reason this waited for `AMB-T-3597` rather than reading the pane's output.
//
// It is also shown at most once per turn. Being on the terminal face *is* being told, so a turn that
// came up while the person was already there never knocks, and one they crossed over to look at does
// not knock again when they come back — the badge answers "something came up while you were away",
// not "something is still standing", which the pane itself says to whoever is looking at it.

/** A turn standing in the terminal, and whether the person has been shown it. */
export type Attention = {
  /** A pane has said it is waiting on a person, and has not gone back to work since. */
  readonly waiting: boolean;
  /** The person has had the terminal face up at some point since that turn came. */
  readonly shown: boolean;
};

/** Nothing is waiting. The state a window starts in, and the one it returns to when the pane goes. */
export const NO_ATTENTION: Attention = { waiting: false, shown: false };

/**
 * Take in what the terminal face says about its panes: whether any of them is waiting on a person,
 * and whether the terminal is the face being looked at as it says so.
 *
 * A turn that is still the same turn does not come round again — `shown` is per turn, so re-hearing
 * one the person has already crossed over to look at must not put the badge back up.
 */
export function turnCame(attention: Attention, waiting: boolean, onTerminal: boolean): Attention {
  if (!waiting) return attention.waiting ? NO_ATTENTION : attention;
  if (attention.waiting) return attention;
  return { waiting: true, shown: onTerminal };
}

/** The person has the terminal face up. Whatever is standing there, they are being shown it. */
export function looked(attention: Attention): Attention {
  return attention.shown ? attention : { ...attention, shown: true };
}

/** Whether the terminal segment should wear its badge. */
export function badgeUp(attention: Attention): boolean {
  return attention.waiting && !attention.shown;
}
