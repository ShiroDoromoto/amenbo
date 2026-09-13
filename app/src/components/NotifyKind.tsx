// What carries a notification, drawn (`AMB-D-885`).
//
// **A kind is said twice — as a ground colour with a glyph on it, and as a word.** A tile alone stops
// being readable once there are four kinds, and a word alone cannot be skimmed down a column. Both
// screens that name a kind take it from here, so a kind added later is one `--k-*` token and one line in
// `KIND_GLYPH`, and neither screen has to be told.
//
// The shelf wears the pair (`screens/NotifyTargetsSetting`); a project's chip wears the dot alone
// (`screens/ProjectNotifySection`), the name beside it being the target's own and the chip being read one
// at a time rather than down a column.
import { t } from "../core/i18n";
import type { NotifyKind } from "../core/notifyTargets";

/** The glyph on a kind's mark. The ground under it is `--k-<kind>`, in `styles/tokens.css`. */
export const KIND_GLYPH: Record<NotifyKind, string> = { slack: "#", mail: "✉" };

/** What a kind is called on screen. */
export function kindLabel(kind: NotifyKind): string {
  return t(`notify.kind.${kind}`);
}

/** The tile and the word — how a kind is told apart in a list of them. */
export function Kind({ kind }: { kind: NotifyKind }) {
  return (
    <span className={`kind k-${kind}`}>
      <span className="kind__tile" aria-hidden="true">{KIND_GLYPH[kind]}</span>
      <span className="kind__name">{kindLabel(kind)}</span>
    </span>
  );
}

/**
 * The tile alone, at the size a chip takes. It carries the kind's name for a reader who is not reading
 * the colour: the glyph is a shape rather than a word, so the word goes on the element itself.
 */
export function KindDot({ kind }: { kind: NotifyKind }) {
  return (
    <span className={`dot k-${kind}`} title={kindLabel(kind)} aria-label={kindLabel(kind)}>
      <span aria-hidden="true">{KIND_GLYPH[kind]}</span>
    </span>
  );
}
