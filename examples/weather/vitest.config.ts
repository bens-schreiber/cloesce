import { defineConfig } from "vitest/config";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { fileURLToPath } from "url";

const resolve = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      "@cloesce": resolve("./.cloesce"),
      "@api": resolve("./src/api"),
    },
  },
  plugins: [
    cloudflareTest({
      main: "./src/api/main.ts",
      wrangler: { configPath: "./wrangler.jsonc" },
    }),
  ],
  test: {
    provide: {
      migrations: await readD1Migrations("./migrations/db"),
    },
  },
});
