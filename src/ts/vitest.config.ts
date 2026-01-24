import { defineConfig } from "vitest/config";
import wasm from "vite-plugin-wasm";

// Consumers of this library do NOT need to install vite-plugin-wasm.
// Vite automatically prebundles npm dependencies (including their WASM imports),
// but it does NOT prebundle local source files during development.
// This plugin is only required so Vitest can load local raw `.wasm` imports.
export default defineConfig({
  plugins: [wasm()],
  test: {
    environment: "node",
  },
});
