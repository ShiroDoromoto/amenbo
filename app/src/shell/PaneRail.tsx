import { useEffect, useRef, useState } from "react";
import type { FrameNames } from "../talk/frames";
import { freeSlot, pageCount, slotsOf, type Layout } from "../talk/layout";
import { t, tf } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";

/**
 * The list of panes beside them: every frame this device has, page by page.
 *
 * **It is the way to a pane that is not on the screen.** Pages are fixed slots and nothing is held
 * back (`../talk/layout`), so the only two ways to a pane are paging to it and picking it here — and
 * this is the one of them that shows what is there to be picked. Rows are grouped by the page they
 * are on because that is the thing a person is about to do: reaching a pane moves the whole screen
 * to its page, and a reader who cannot see that coming loses the pane they were watching.
 *
 * **The rail is where a person names a pane.** A name belongs to the frame and a person's word is the
 * last one (`../talk/frames`), so the rename is here rather than on the pane: it is the one place a
 * frame with nothing running in it can still be named.
 *
 * **And it is where another pane is started in a project already open.** A page is one project
 * (`../talk/layout`), so a page that has a folder has everywhere a new pane needs: pressing the way in
 * beside its name opens one there and nothing is asked. It is the whole difference between the second
 * pane in a project and the first — the first is a folder being chosen, and every one after it is a
 * press.
 */
export function PaneRail({
  layout, names, onPick, onRename, onOpen,
}: {
  layout: Layout;
  names: FrameNames;
  onPick: (frame: string) => void;
  onRename: (frame: string, name: string) => void;
  /** Start a pane in the first free slot of this page. A page with no free slot does not offer it. */
  onOpen: (page: number) => void;
}) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    field.current?.select();
  }, [renaming]);

  const pages = Array.from({ length: pageCount(layout) }, (_, i) => i + 1);

  return (
    <nav className="rail" aria-label={t("face.rail")}>
      {pages.map((page) => (
        <div className="rail__page" key={page}>
          <div className="rail__pagerow">
            <span className="rail__pagename">{tf("face.page", { n: page })}</span>
            {/* Only where there is somewhere to put one. A page whose slots are all taken has nothing
                this could do, and a control that cannot do anything is one a reader learns to ignore.
                There is always at least one page with room: the arrangement offers one more page than
                the frames fill. */}
            {freeSlot(layout, page) !== null && (
              <button
                className="rail__open"
                title={t("face.openHere")}
                aria-label={t("face.openHere")}
                onClick={() => onOpen(page)}
              >
                +
              </button>
            )}
          </div>
          {slotsOf(layout, page).map((frame, slot) => {
            // A slot no frame has been made for yet has nothing to name and nothing to go to: the
            // page is what leads there, and pressing it is what makes the frame.
            if (!frame) return <div className="rail__slot" key={`${page}.${slot}`} />;
            const name = names.get(frame.id) ?? null;
            return renaming === frame.id
              ? (
                <input
                  key={frame.id}
                  ref={field}
                  className="rail__rename"
                  defaultValue={name ?? ""}
                  autoFocus
                  aria-label={t("face.rename")}
                  {...asTyped}
                  onKeyDown={(e) => {
                    if (isEnterSubmit(e)) {
                      e.preventDefault();
                      const text = e.currentTarget.value.trim();
                      if (text) onRename(frame.id, text);
                      setRenaming(null);
                    }
                    if (e.key === "Escape") setRenaming(null);
                  }}
                  onBlur={() => setRenaming(null)}
                />
              )
              : (
                <button
                  key={frame.id}
                  className={`rail__row${layout.focus === frame.id ? " rail__row--focused" : ""}`}
                  onClick={() => onPick(frame.id)}
                  onDoubleClick={() => setRenaming(frame.id)}
                  title={t("face.rename")}
                >
                  {/* A frame with no name is still a place, and the place is what it is called until
                      someone says otherwise — the number is the slot, counted the way the page is. */}
                  <span className="rail__name">{name ?? `${page}.${slot + 1}`}</span>
                  {frame.session === null && <span className="rail__idle">·</span>}
                </button>
              );
          })}
        </div>
      ))}
    </nav>
  );
}
