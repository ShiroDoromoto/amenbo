import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite dev server. `npm run dev` stays a plain browser launch (for iterating on the front end alone), while
// `npm run tauri dev` drives the same server (Tauri waits on devUrl=:5180, hence strictPort).
export default defineConfig({
  plugins: [react()],
  // Tauri assumes a fixed port (get it wrong and you get a white screen).
  server: { port: 5180, strictPort: true },
  clearScreen: false,
});
