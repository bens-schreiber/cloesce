import { defineConfig } from "vitest/config";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { fileURLToPath } from "url";

const resolve = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      "@cloesce": resolve("./.cloesce"),
    },
  },
  plugins: [
    cloudflareTest({
      main: "./src/api/main.ts",
      wrangler: { configPath: "./wrangler.toml" },
    }),
  ],
  test: {
    include: ["tests/**/*.test.ts"],
    provide: {
      migrations: await readD1Migrations("./migrations/Db"),
    },
  },
});
