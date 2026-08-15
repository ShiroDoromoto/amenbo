// The attachment view for a task or a decision record. The renderer is chosen by mime (image, audio,
// video, PDF, text, markdown, CSV), and anything large streams through the custom protocol (blobUrl)
// rather than a data URL. A url-mode attachment is a link, so it opens; a blob is a file, so it
// downloads — every blob carries that button, previewable or not, because a preview is not a copy
// the user keeps. There are two ways to add one: the file picker (by path, ingested as a stream) and
// drag & drop (by bytes).
//
// Which renderer runs is decided by an allowlist (`previewKind`). An attachment is unverified bytes
// and the webview is an execution environment with a line to the IPC, so the types we will render are
// enumerated, and anything executable (SVG, HTML, XML…) is downgraded to a source view.

import { useEffect, useState } from "react";
import { useAttachments, type Attachment } from "../core/reads";
import { blobUrl } from "../core/blobUrl";
import {
  attachDroppedFiles, openAttachment, pickAndAttach, removeAttachment, saveAttachment,
  type AttachTarget,
} from "../core/mutations";
import { confirmDialog } from "../core/dialog";
import { previewKind } from "../core/attachmentView";
import { Markdown } from "./Markdown";
import { formatNumber, t, tf } from "../core/i18n";
import { Icon } from "./Icon";

// A size a reader can take in: the largest unit the byte count fills, to one decimal while that
// decimal still says something. The digits go through `Intl` because the separator is the locale's —
// half a megabyte is `0.5 MB` in English and `0,5 MB` in German.
function humanSize(n: bigint | number | null): string {
  if (n === null) return "";
  const bytes = Number(n);
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  const digits = v >= 10 ? 0 : 1;
  const size = formatNumber(v, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return `${size} ${units[i]}`;
}

/** The preview itself, dispatched on mime. A missing blob, a url-mode attachment and an unsupported type each get their own display. */
function AttachmentBody({ a }: { a: Attachment }) {
  if (a.kind === "url") {
    return (
      <div className="attach__url">
        🔗 <a href={a.url ?? "#"} onClick={(e) => { e.preventDefault(); if (a.url) void openAttachment(a.url); }}>
          {a.filename || a.url}
        </a>
      </div>
    );
  }
  if (!a.present || !a.blobHash) {
    return <div className="attach__missing faint">{t("attach.notLocal")}</div>;
  }
  const src = blobUrl(a.blobHash, a.mime);
  switch (previewKind(a.mime)) {
    case "image":
      return <img className="attach__img" src={src} alt={a.filename ?? ""} loading="lazy" />;
    case "audio":
      return <audio className="attach__audio" src={src} controls preload="metadata" />;
    case "video":
      return <video className="attach__video" src={src} controls preload="metadata" />;
    // A PDF is the one type the webview renders **as a document**, so it is fenced into a sandbox with
    // no tokens: an opaque origin, no scripts. A bare <iframe> here would let an attachment become a
    // document running in our own origin.
    case "pdf":
      return <iframe className="attach__pdf" src={src} title={a.filename ?? "pdf"} sandbox="" />;
    case "markdown":
      return <TextBody src={src} render="markdown" />;
    case "csv":
      return <TextBody src={src} render="csv" sep="," />;
    case "tsv":
      return <TextBody src={src} render="csv" sep={"\t"} />;
    case "text":
      return <TextBody src={src} render="text" />;
    // No preview, and no "open externally" either: the header's download button is the one way out,
    // and it leaves the file where the user chose rather than in a temp dir they cannot find.
    case "none":
      return <div className="attach__unsupported faint">{t("attach.unsupported")}</div>;
  }
}

/** Fetches text/markdown/CSV over the protocol and renders it, truncating a large file at CAP so it cannot run away. */
function TextBody({ src, render, sep }: { src: string; render: "text" | "markdown" | "csv"; sep?: string }) {
  const [text, setText] = useState<string | null>(null);
  const [err, setErr] = useState(false);
  const CAP = 200_000; // character cap, so a huge text file cannot freeze the view.
  useEffect(() => {
    let alive = true;
    setText(null); setErr(false);
    fetch(src)
      .then((r) => r.text())
      .then((s) => { if (alive) setText(s.length > CAP ? s.slice(0, CAP) + "\n…" : s); })
      .catch(() => { if (alive) setErr(true); });
    return () => { alive = false; };
  }, [src]);
  if (err) return <div className="attach__missing faint">{t("attach.notLocal")}</div>;
  if (text === null) return <div className="faint attach__loading">…</div>;
  if (render === "markdown") return <div className="attach__text markdown"><Markdown>{text}</Markdown></div>;
  if (render === "csv") return <CsvTable text={text} sep={sep ?? ","} />;
  return <pre className="attach__text attach__pre">{text}</pre>;
}

/** Renders CSV/TSV as a plain table (the first MAX_ROWS rows), splitting naively and ignoring quotes — this is a preview. */
function CsvTable({ text, sep }: { text: string; sep: string }) {
  const MAX_ROWS = 200;
  const rows = text.split(/\r?\n/).filter((l) => l.length > 0).slice(0, MAX_ROWS).map((l) => l.split(sep));
  if (rows.length === 0) return <pre className="attach__text attach__pre">{text}</pre>;
  const [head, ...body] = rows;
  return (
    <div className="attach__csvwrap">
      <table className="attach__csv">
        <thead><tr>{head.map((c, i) => <th key={i}>{c}</th>)}</tr></thead>
        <tbody>{body.map((r, ri) => <tr key={ri}>{r.map((c, ci) => <td key={ci}>{c}</td>)}</tr>)}</tbody>
      </table>
    </div>
  );
}

/**
 * The attachment view, used both on the body of a task or decision record (`compact` omitted) and
 * under a single comment (`compact`). The compact form is the space-saving one that slots in beneath
 * a comment: it drops the "Attachments" heading and folds into a 📎 add button, so it reads as
 * distinct from a body attachment — by its nesting and by the absent heading — and attachments can be
 * followed along the comment timeline.
 */
export function Attachments({ target, targetId, compact = false }: {
  target: AttachTarget;
  targetId: number;
  compact?: boolean;
}) {
  const attachments = useAttachments(target, targetId);
  const [dragActive, setDragActive] = useState(false);
  const [busy, setBusy] = useState(false);

  const onDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setDragActive(false);
    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;
    setBusy(true);
    try { await attachDroppedFiles(target, targetId, files); }
    finally { setBusy(false); }
  };
  const onPick = async () => {
    setBusy(true);
    try { await pickAndAttach(target, targetId); }
    finally { setBusy(false); }
  };
  const onRemove = async (a: Attachment) => {
    const name = a.filename || a.url || "";
    if (await confirmDialog(tf("attach.removeConfirm", { name }))) {
      await removeAttachment(a.id, target, targetId);
    }
  };

  const items = attachments.map((a) => (
    <div className="attach__item" key={a.id}>
      <div className="attach__head">
        <span className="attach__name" title={a.filename ?? a.url ?? ""}>
          {a.kind === "url" ? "🔗" : "📎"} {a.filename || a.url || "(no name)"}
        </span>
        <span className="attach__meta faint">
          {a.kind === "blob" ? humanSize(a.sizeBytes) : t("attach.link")}
        </span>
        {a.kind === "blob" && a.present && a.blobHash && (
          <button
            className="feed__action attach__dl"
            title={t("attach.download")}
            onClick={() => void saveAttachment(a.blobHash!, a.filename)}
          >
            ⬇
          </button>
        )}
        <button className="feed__action attach__rm" title={t("attach.remove")} onClick={() => void onRemove(a)}><Icon name="close" /></button>
      </div>
      <AttachmentBody a={a} />
    </div>
  ));

  if (compact) {
    return (
      <div
        className={`attach__drop attach__drop--compact ${dragActive ? "attach__drop--active" : ""}`}
        onDragOver={(e) => { e.preventDefault(); setDragActive(true); }}
        onDragLeave={() => setDragActive(false)}
        onDrop={onDrop}
      >
        {items.length > 0 && <div className="attach__list">{items}</div>}
        {dragActive ? (
          <div className="faint attach__drophint">{t("attach.dropActive")}</div>
        ) : (
          <button className="feed__action attach__compactadd" disabled={busy} onClick={onPick}>
            📎 {t("attach.add")}
          </button>
        )}
      </div>
    );
  }

  return (
    <div>
      <div className="detail__section-h">
        {t("attach.section")}
        <button className="feed__action" style={{ marginLeft: 8 }} disabled={busy} onClick={onPick}>
          ＋ {t("attach.add")}
        </button>
      </div>
      <div
        className={`attach__drop ${dragActive ? "attach__drop--active" : ""}`}
        onDragOver={(e) => { e.preventDefault(); setDragActive(true); }}
        onDragLeave={() => setDragActive(false)}
        onDrop={onDrop}
      >
        {attachments.length === 0 ? (
          <div className="faint attach__drophint">
            {dragActive ? t("attach.dropActive") : `${t("attach.dropHint")} `}
            {!dragActive && <button className="feed__action" disabled={busy} onClick={onPick}>{t("attach.add")}</button>}
          </div>
        ) : (
          <div className="attach__list">
            {items}
            {dragActive && <div className="faint attach__drophint">{t("attach.dropActive")}</div>}
          </div>
        )}
      </div>
    </div>
  );
}
