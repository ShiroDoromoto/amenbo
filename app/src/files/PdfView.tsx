// A PDF drawn where it is shown rather than handed to another application (`AMB-D-907`) — the file
// panel's, and an attachment's on a record (`../components/Attachments`).
//
// What it draws with arrives asynchronously and can fail to arrive, so a failure here is the
// caller's to say: in the panel the sentence and the way on are the ones a file that could not be
// read already gets, and this only tells it which of the two happened (`./FilesPanel`).

import { useEffect, useRef } from "react";
import { tf } from "../core/i18n";
import { mountPdf, type MountedPdf } from "./pdfLoad";

/** One PDF, drawn from a door of ours — a file by its path, or an attachment by its hash. */
export function PdfView({ src, onFailed, scrolls }: {
  /** The address the pages are fetched from (`../core/fileUrl`, `../core/blobUrl`). */
  src: string;
  /** Told where the document could not be opened at all. */
  onFailed: () => void;
  /** What scrolls the document past the reader, where it is not the panel's body (`./pdfLoad`). */
  scrolls?: string;
}) {
  const host = useRef<HTMLDivElement | null>(null);
  // The callback as it is now, so a panel that re-renders does not take it as a reason to open the
  // document again.
  const told = useRef(onFailed);
  told.current = onFailed;

  useEffect(() => {
    const parent = host.current;
    if (parent === null) return;
    let drawn: MountedPdf | null = null;
    let gone = false;
    void (async () => {
      try {
        const mounted = await mountPdf(
          parent, src, (page, of) => tf("files.pdfPage", { page, of }), scrolls,
        );
        // The reader closed the file while it was opening: what arrived is taken down rather than
        // left drawing into an element nobody is looking at.
        if (gone) mounted.close();
        else drawn = mounted;
      } catch {
        if (!gone) told.current();
      }
    })();
    return () => {
      gone = true;
      drawn?.close();
    };
    // The address carries the file's own mark, so a file rewritten under the reader is a new address
    // and a document opened again (`AMB-D-797`).
  }, [src, scrolls]);

  return <div className="files__pdf" ref={host} />;
}
