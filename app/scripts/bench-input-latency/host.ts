// A stand-in for the Tauri host, so the app's own reads run as they do in the shell.
//
// `inTauri()` only asks whether `__TAURI_INTERNALS__` is there, and every command goes out through
// its `invoke` (`src/core/ipc.ts`). Answering the two reads a run's pane makes — the automation, for
// the box its step is numbered by, and the project's roles — is what lets the real roles be drawn
// here; anything else is answered with nothing, and noted, so a read the panes
// start making later shows up in the bench's output instead of passing unseen.
import { automation, roles } from "./fixture";

const unknown = new Set<string>();
let callbacks = 0;

function answer(cmd: string): unknown {
  switch (cmd) {
    case "automation_detail":
      return automation;
    case "project_roles":
      return roles;
    case "pty_sessions":
      return [];
    case "skin_in_use":
      // The default skin, which is what the app draws with until one is chosen.
      return null;
    default:
      if (!cmd.startsWith("plugin:")) unknown.add(cmd);
      return null;
  }
}

Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke: (cmd: string) => Promise.resolve(answer(cmd)),
    transformCallback: () => ++callbacks,
    unregisterCallback: () => {},
    convertFileSrc: (path: string) => path,
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
});

(window as unknown as { __bench: { unknown: () => string[] } }).__bench.unknown = () => [...unknown].sort();
