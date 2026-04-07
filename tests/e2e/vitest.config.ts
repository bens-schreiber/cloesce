import { defineConfig } from "vitest/config";
import wasm from "vite-plugin-wasm";

function jsoncPlugin() {
  return {
    name: "jsonc",
    transform(code: string, id: string) {
      if (!id.endsWith(".jsonc")) return null;
      return { code: `export default ${code}`, map: null };
    },
  };
}

export default defineConfig({
  plugins: [wasm(), jsoncPlugin()],
});
