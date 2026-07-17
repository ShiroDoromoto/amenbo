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
}

const RefNavContext = createContext<RefNav>({});

export function RefNavProvider({ value, children }: { value: RefNav; children: ReactNode }) {
  return <RefNavContext.Provider value={value}>{children}</RefNavContext.Provider>;
}

export function useRefNav(): RefNav {
  return useContext(RefNavContext);
}
