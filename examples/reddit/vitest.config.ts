import { defineConfig } from "vitest/config";
import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { fileURLToPath } from "url";

const resolve = (p: string) => fileURLToPath(new URL(p, import.meta.url));

// Tests run *inside* the Workers runtime via @cloudflare/vitest-pool-workers.
// This is what lets a backend test reach Durable Object code directly (with the
// real DO storage context) through `runInDurableObject`, without any HTTP or the
// generated frontend client. Bindings (DOs, KV, R2) are read from wrangler.jsonc.
//
// The pool's bundler doesn't read tsconfig `paths`, so the `@cloesce`/`@api`
// aliases used by the app code are declared here too.
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
});
