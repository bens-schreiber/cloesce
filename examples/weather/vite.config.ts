import { defineConfig } from "vite";
import tsconfigPaths from "vite-tsconfig-paths";
import { cloesce } from "cloesce/vite";

export default defineConfig({
    plugins: [tsconfigPaths(), cloesce()],
    root: "./src/web"
});
