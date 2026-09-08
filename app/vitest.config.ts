import { defineConfig } from "vitest/config";

// Deliberately does not inherit vite.config.ts's dev-server settings — this file takes precedence.
// The default environment is node (pure logic); component tests that need a DOM opt in per file with
// a leading `// @vitest-environment jsdom`.
export default defineConfig({
  // Parity tests pull Rust sources in with `?raw` from outside `app/`. Under the node environment that is served
  // straight off disk, but a jsdom test goes through the fs-allow check and is denied — so name the repo root.
  server: { fs: { allow: [".."] } },
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx}"],
    // What jsdom does not implement and the interface needs anyway (`src/vitest.setup.ts`). It is
    // named here rather than imported per file so a component test cannot forget it and fail on the
    // browser rather than on itself.
    setupFiles: ["./src/vitest.setup.ts"],
  },
});
