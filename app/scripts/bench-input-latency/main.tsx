// The page the bench types into: a workspace page of real panes (`src/shell/TerminalPane.tsx`), on
// the app's own stylesheets, drawn the way the workspace draws them.
//
// What is put in place of the app is only what reaches the host: the terminal (`./agent`), the row
// over a pane (`./plate`), files dropped from the desktop (`./hostDrop`) and the commands (`./host`).
// Everything a key pressed in the box sets going is the app's: the page holds every pane's draft, so
// a key draws the page again with every handler and every run made afresh, as the workspace does.
//
// `?panes=1|4` is how many panes there are; `?mode=terminal|picture` is whether they are ordinary
// panes or runs' panes turned to their pictures.
import "./host";
import "@app/styles/tokens.css";
import "@app/styles/global.css";
import "@app/components/components.css";
import "@app/styles/utilities.css";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { initTheme } from "@app/core/theme";
import { initSkin } from "@app/core/skin";
import { TerminalPane } from "@app/shell/TerminalPane";
import { say } from "./fixture";

const query = new URLSearchParams(location.search);
const PANES = Number(query.get("panes") ?? "1");
const PICTURE = query.get("mode") === "picture";
const frames = Array.from({ length: PANES }, (_, i) => `pane-${i + 1}`);
const names = new Map();

/** Where each pane sits on the page's twelve-by-two grid: one pane fills it, four take a quarter. */
function at(i: number) {
  if (PANES === 1) return { gridColumn: "1 / span 12", gridRow: "1 / span 2" };
  return { gridColumn: `${(i % 2) * 6 + 1} / span 6`, gridRow: `${Math.floor(i / 2) + 1} / span 1` };
}

/** Whether every pane has its box, and every picture pane its picture with the run's box in it. */
function drawn(): boolean {
  if (document.querySelectorAll(".compose__box").length < PANES) return false;
  if (!PICTURE) return true;
  return document.querySelectorAll(".runpic").length === PANES
    && document.querySelectorAll(".runpic .autopic__node").length > PANES;
}

function Page() {
  const [written, setWritten] = useState<Record<string, string>>({});
  useEffect(() => {
    const look = setInterval(() => {
      if (!drawn()) return;
      clearInterval(look);
      (window as unknown as { __bench: { ready: boolean } }).__bench.ready = true;
    }, 50);
    return () => clearInterval(look);
  }, []);
  const none = () => {};
  return (
    <div style={{ display: "flex", height: "100vh" }}>
      <div className="workspace__page-grid">
        {frames.map((frame, i) => (
          <TerminalPane
            key={frame}
            frame={frame}
            at={at(i)}
            hue={199}
            project={1}
            names={names}
            start={{ frame, cwd: `/work/${frame}` }}
            run={PICTURE ? say(100 + i) : null}
            autoStart
            focused={i === 0}
            written={written[frame] ?? ""}
            onWrite={(id: string, text: string) => setWritten((was) => ({ ...was, [id]: text }))}
            composeOpen
            onFold={none}
            onOpened={none}
            onSaid={none}
            onPath={none}
            onClosed={none}
            onDrop={none}
            onName={none}
            onFocus={none}
          />
        ))}
      </div>
    </div>
  );
}

initTheme();
initSkin();
createRoot(document.getElementById("root")!).render(<Page />);
