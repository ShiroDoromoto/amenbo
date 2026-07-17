import { defineConfig } from "vitest/config";

// Deliberately does not inherit vite.config.ts's dev-server settings — this file takes precedence.
// The default environment is node (pure logic); component tests that need a DOM opt in per file with
// a leading `// @vitest-environment jsdom`.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
