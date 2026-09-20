// The "made in" row on a task's or a decision's detail pane: the pane it was made in, and the press
// that goes back into it (`AMB-D-897`).
//
// **The question it answers is "which session made this?"** — asked by somebody reading a decision
// nobody remembers writing. The pane is what can be gone back to; the name is what is left to read
// where it cannot.
//
// **The name is preferred live.** A pane still open in this run may have been renamed since the
// record was written, and the name on the screen is the one the reader can find among the panes
// (`crate::frames::frame_names`). What the record holds is what it was called then, which is what a
// pane that is gone leaves behind.
//
// **What pressing does is not decided here, and cannot be decided before it is pressed.** A pane
// that is open is gone to; one that is not is opened again under the same id, on the way back the
// record holds — and whether that way back still leads to a conversation is the provider's answer,
// which arrives seconds later as the pane either comes up or stops (`AMB-D-897`,
// `app/src/shell/TerminalPane.tsx`).
//
// **A pane opened again is asked where it works and what to run, the way every pane is**
// (`app/src/shell/EmptySlot.tsx`). The record names neither: it holds the pane, its name and the way
// back, and never the folder or the provider (`amenbo_core::model::TaskMadeIn`). Answering either
// from somewhere else would be Amenbo guessing at the conversation it is trying to reopen.

import { useEffect, useState } from "react";
import type { FrameNameDto, TalkLayoutDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { t } from "../core/i18n";
import type { MadeIn as MadeInRow } from "../core/reads";
import { inTauri } from "../core/snapshot";
import { Icon } from "./Icon";

export function MadeIn({
  made,
  project,
  openAgain,
  onGoToPane,
}: {
  /** The pane the record was made in, as it was written down. */
  made: MadeInRow;
  /** The project the pane belongs to when it has to be opened again — the record's own. */
  project: number;
  /** Put the record's way back on that frame, host-side, before it is opened again. */
  openAgain: () => Promise<void>;
  /** Go to the workspace, at that pane. */
  onGoToPane: (project: number, pane: string) => void;
}) {
  // What the pane is called now, where it is still open. Asked once — a name changes while a pane is
  // open, and a row that followed every change would be redrawing a label nobody is looking at.
  const [live, setLive] = useState<string | null>(null);
  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    void invoke<FrameNameDto[]>("frame_names")
      .then((names) => {
        if (alive) setLive(names.find((one) => one.frame === made.pane)?.name ?? null);
      })
      .catch(() => {});
    return () => { alive = false; };
  }, [made.pane]);

  const press = async () => {
    // Whether the pane is on the screen is the arrangement's answer, and this run holds it
    // (`crate::frames::talk_layout`). A pane that is not in it is one to open again.
    const layout = inTauri() ? await invoke<TalkLayoutDto | null>("talk_layout") : null;
    const open = layout?.frames.some((one) => one.id === made.pane) ?? false;
    if (!open) await openAgain();
    onGoToPane(project, made.pane);
  };

  return (
    <div>
      <div className="detail__section-h">{t("madeIn.section")}</div>
      <button className="btn" title={t("madeIn.go")} onClick={() => void press()}>
        <Icon name="keyboard" />
        <span>{live ?? made.paneName ?? t("madeIn.unnamed")}</span>
      </button>
    </div>
  );
}
