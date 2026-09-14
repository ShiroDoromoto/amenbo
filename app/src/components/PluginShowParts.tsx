// **What a plugin's author asked to have drawn on its settings form** (`AMB-D-727`), drawn.
//
// Everything here arrives as strings and leaves as Amenbo's own paint. There is no markup to interpret,
// no image to decode and no layout to honour: a `qr` is the text to encode and the squares are drawn
// here, a `link` is a destination and the words on a button. That line is what lets somebody standing in
// front of a form asking for a credential still tell Amenbo's window from a stranger's writing in it —
// and it is why a plugin stays a child process instead of a webview per platform (`AMB-D-346`).
//
// Core has already settled what may reach this: the vocabulary, the caps, and the rule that a
// destination is an official plugin's alone. So this file draws what it is handed and asks nothing.
import { useState } from "react";
import type { PluginShowPartDto } from "../bindings/bindings";
import { QrCode } from "./QrCode";
import { t } from "../core/i18n";
import { openExternalUrl } from "../core/mutations";

/**
 * The parts one run — or one manifest — asked to have drawn, in the order they were written.
 *
 * Nothing at all is nothing drawn, which is every plugin written before this vocabulary existed and
 * every run that had only its one line to say.
 */
export function PluginShowParts({ parts }: { parts: PluginShowPartDto[] }) {
  if (parts.length === 0) return null;
  return (
    <div className="plugshow">
      {parts.map((part, i) => (
        // The index is the identity: a part is a position in a list its author wrote, with no key of its
        // own, and the whole list is replaced at once whenever it changes.
        <PluginShowPart key={i} part={part} />
      ))}
    </div>
  );
}

/** One part, drawn as what it is. */
function PluginShowPart({ part }: { part: PluginShowPartDto }) {
  switch (part.kind) {
    case "text":
      return <p className="plugshow__text">{part.text}</p>;
    case "heading":
      return <div className="plugshow__heading">{part.text}</div>;
    case "note":
      return <div className="plugshow__note">{part.text}</div>;
    case "list":
      return (
        <ul className="plugshow__list">
          {part.items.map((line, i) => (
            <li key={i}>{line}</li>
          ))}
        </ul>
      );
    case "copy":
      return <ShowCopy text={part.text} />;
    case "qr":
      return <ShowQr text={part.text} />;
    case "link":
      // The author's own words on the button, and their destination behind it. It goes out through the
      // one door the app opens a browser with, which is a person pressing a button and not Amenbo
      // reaching the network.
      return (
        <button className="btn" onClick={() => void openExternalUrl(part.url)}>
          {part.label}
        </button>
      );
  }
}

/**
 * A string with a copy button beside it — what nobody should have to retype, and the third party's way
 * of naming a destination: it is read before it is followed, which is the whole difference from a `link`
 * (`AMB-D-727`).
 */
function ShowCopy({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };
  return (
    <div className="plugshow__copy">
      <code>{text}</code>
      <button className="btn" onClick={() => void copy()}>
        {copied ? t("plugins.show.copied") : t("plugins.show.copy")}
      </button>
    </div>
  );
}

/**
 * A QR code drawn from the author's string.
 *
 * This is the part the vocabulary exists for: `viewer` was writing a PNG to a file and asking the
 * operating system to open it, which fails quietly on a machine with nothing registered for the type,
 * and leaves the reader looking at a settings form where nothing happened. The drawing itself is shared
 * with the Viewer's own settings (`components/QrCode`).
 */
function ShowQr({ text }: { text: string }) {
  return <QrCode text={text} label={t("plugins.show.qr")} className="qrcode" />;
}
