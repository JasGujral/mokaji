import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// REL-3: no network fetch is required to render the UI. Fonts are vendored through
// @fontsource (npm), never linked from Google Fonts — a HUD that needs the internet to draw
// its own text is not a local-first HUD.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", sourcemap: true },
});
