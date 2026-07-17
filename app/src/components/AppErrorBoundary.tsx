import { Component, type ErrorInfo, type ReactNode } from "react";
import { t } from "../core/i18n";

/**
 * The app-wide backstop. A synchronous throw in any child's render would otherwise unmount the whole
 * React tree and leave a black screen (the body's dark background and nothing else); this catches it
 * and shows a fallback with a way back. It wraps the root (`<App/>` in main.tsx), so even the first
 * render at startup is covered.
 */
export class AppErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Keep the full trace as a debugging lead (it also reaches the DevTools console in release); the UI only gets a summary.
    console.error("AppErrorBoundary caught a render error:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    // The fallback is localized and styled inline, so it depends on no CSS or context from below the boundary.
    return (
      <div
        role="alert"
        style={{
          maxWidth: 560,
          margin: "12vh auto",
          padding: 24,
          fontFamily: "system-ui, sans-serif",
          color: "#e6e3dc",
          background: "#232019",
          border: "1px solid #3a352b",
          borderRadius: 10,
          lineHeight: 1.6,
        }}
      >
        <strong style={{ fontSize: 16 }}>{t("app.crashTitle")}</strong>
        <p style={{ margin: "8px 0 16px", opacity: 0.8 }}>{t("app.crashHint")}</p>
        <button
          type="button"
          onClick={() => window.location.reload()}
          style={{
            padding: "8px 16px",
            fontSize: 14,
            color: "#1b1a17",
            background: "#c8b98f",
            border: "none",
            borderRadius: 6,
            cursor: "pointer",
          }}
        >
          {t("app.crashReload")}
        </button>
        {this.state.error.message && (
          <pre
            style={{
              marginTop: 16,
              padding: 12,
              maxHeight: 160,
              overflow: "auto",
              fontSize: 12,
              opacity: 0.6,
              whiteSpace: "pre-wrap",
              background: "#1b1a17",
              borderRadius: 6,
            }}
          >
            {this.state.error.message}
          </pre>
        )}
      </div>
    );
  }
}
