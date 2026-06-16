import { defineConfig } from "vite";
import { fileURLToPath } from "url";

export default defineConfig({
  root: "./src/web",
  resolve: {
    alias: { "@cloesce": fileURLToPath(new URL("./.cloesce", import.meta.url)) },
  },
});
