// The navigation seam for links inside body text.
//
// Clicking a reference detected in notes, comments or a decision body (`AMB-T-<n>` / `AMB-D-<n>`) switches the
// right pane to that task or decision. The switching itself (selectTask/selectDecision) is held by AppShell, so
// rather than passing props down the deep React tree where the Markdown is rendered, we supply it from one place
// via Context.
// Outside the provider (tests, previews) the default is `{}` and clicking a link is a no-op.
import { createContext, useContext, type ReactNode } from "react";

export interface RefNav {
  selectTask?: (id: number) => void;
  selectDecision?: (id: number | null) => void;
  /** Open one automation's build screen, with the box a run stopped at already pressed where one is
   *  named (`AMB-T-5539`) — from a run's pane, which is the workspace's and not the ledger's. */
  openAutomation?: (project: number, automation: number, placement: number | null) => void;
  /** Open a project's automations on the "history" tab — from a run's pane, once the run is over. */
  openRunHistory?: (project: number) => void;
  /** Bring the workspace forward — from a launch refused because it is closed (`AMB-T-5590`). The
   *  press lands wherever the workspace is: this window's face, or the window it was split out into. */
  openWorkspace?: () => void;
}

const RefNavContext = createContext<RefNav>({});

export function RefNavProvider({ value, children }: { value: RefNav; children: ReactNode }) {
  return <RefNavContext.Provider value={value}>{children}</RefNavContext.Provider>;
}

export function useRefNav(): RefNav {
  return useContext(RefNavContext);
}
