import { exec } from "child_process";
import { promisify } from "util";
import type { Plugin, ViteDevServer } from "vite";

const execAsync = promisify(exec);

export interface CloescePluginOptions {
    /**
     * File path patterns that trigger recompilation.
     * Defaults to all files (empty array = match all).
     * Matching is performed as a simple substring check on the full file path.
     * For example, `["/data/"]` would also match `/metadata/file.ts`.
     * @default []
     */
    include?: string[];
    /**
     * Additional directories outside Vite's root to watch for changes.
     * Useful when your .cloesce.ts files live outside the Vite root.
     * @default ["src/data"]
     */
    watchDirs?: string[];
}

/**
 * Vite plugin that automatically runs `cloesce compile` on dev server start
 * and whenever files change.
 *
 * @example
 * ```ts
 * import { defineConfig } from "vite";
 * import { cloesce } from "cloesce/vite";
 *
 * export default defineConfig({
 *     plugins: [cloesce()],
 * });
 * ```
 */
export function cloesce(options: CloescePluginOptions = {}): Plugin {
    const include = options.include ?? [];
    const watchDirs = options.watchDirs ?? ["src/data"];
    let isCompiling = false;

    return {
        name: "cloesce-compile",

        configureServer(server: ViteDevServer) {
            for (const dir of watchDirs) {
                server.watcher.add(dir);
            }
        },

        async hotUpdate({ file, server }: { file: string; server: ViteDevServer }) {
            if (include.length > 0 && !include.some((pattern) => file.includes(pattern))) {
                return;
            }
            if (isCompiling) {
                return;
            }
            isCompiling = true;
            server.config.logger.info("[cloesce] Compiling...", { timestamp: true });
            try {
                const { stdout, stderr } = await execAsync("npx cloesce compile");
                if (stdout) server.config.logger.info(stdout);
                if (stderr) server.config.logger.warn(stderr);
                server.config.logger.info("[cloesce] Compile completed", { timestamp: true });
            } catch (error: any) {
                server.config.logger.error(`[cloesce] Compile failed: ${error.message}`);
                if (error.stdout) server.config.logger.error(error.stdout);
                if (error.stderr) server.config.logger.error(error.stderr);
            } finally {
                isCompiling = false;
            }
        },

        async buildStart() {
            if (isCompiling) {
                return;
            }
            isCompiling = true;
            this.warn("[cloesce] Running initial compile...");
            try {
                const { stdout, stderr } = await execAsync("npx cloesce compile");
                if (stdout) this.warn(stdout);
                if (stderr) this.warn(stderr);
                this.warn("[cloesce] Initial compile completed");
            } catch (error: any) {
                this.warn(`[cloesce] Initial compile failed: ${error.message}`);
                if (error.stdout) this.warn(error.stdout);
                if (error.stderr) this.warn(error.stderr);
            } finally {
                isCompiling = false;
            }
        },
    };
}

export default cloesce;
