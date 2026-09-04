import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/tokens.css";
import "./styles/global.css";
import "./components/components.css";
import "./styles/utilities.css";
import { initTheme } from "./core/theme";
import App from "./App";
import { AppErrorBoundary } from "./components/AppErrorBoundary";

initTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AppErrorBoundary>
      <App />
    </AppErrorBoundary>
  </StrictMode>,
);
