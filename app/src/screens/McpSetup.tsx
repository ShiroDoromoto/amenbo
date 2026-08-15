// The line on a project's settings screen that says an AI can be connected, and where (`AMB-D-681`).
//
// It stands there because that is where a reader is thinking about one project at all. What it does
// **not** do is set anything up: a server is one per app and reaches as many projects as the reader
// chose (`AMB-D-679`), so the choosing happens once, on the screen this points at, rather than once per
// project on a screen that would have to work out what the other projects had already asked for.
//
// **The screen that just made a project carries a plain line instead** (`AMB-D-684`): a reader who has
// only now raised one cannot yet say whether their AI opens folders, so there is nothing there for a
// fold to ask them.
//
// **Folded away, because most readers do not need it.** Somebody working from the command line has
// amenbo already; the offer is for the reader whose AI cannot open a folder at all. Folded, it is a
// line they can walk past.
import { useState } from "react";
import { t } from "../core/i18n";

/** `onOpen` takes the reader to the screen where apps are connected. */
export function McpSetup({ onOpen }: { onOpen: () => void }) {
  const [open, setOpen] = useState(false);

  return (
    <div className="mcpsetup">
      {/* Its own line, not the MCP screen's heading: this one names the subject a reader is thinking
          about here — one project, reached from an AI — while the heading over there names the road
          the sidebar sends them down. */}
      <div className="fieldlabel">{t("mcp.setupTitle")}</div>
      {/* A disclosure rather than a link straight out: what is behind it is one sentence saying why
          the road leaves this screen, and a reader who never wanted it never reads that either. */}
      <button className="btn mcpsetup__toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        {open ? "▾" : "▸"} {t("mcp.open")}
      </button>

      {open && (
        <>
          <span className="hint">{t("mcp.hint")}</span>
          <button className="btn" onClick={onOpen}>{t("nav.mcp")}</button>
        </>
      )}
    </div>
  );
}
