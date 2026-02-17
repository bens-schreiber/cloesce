import { exec } from "child_process";
import { promisify } from "util";

const execAsync = promisify(exec);

export interface CloescePluginOptions {
    /**
     * File path patterns that trigger recompilation.
     * Defaults to all files (empty array = match all).
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
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function cloesce(options: CloescePluginOptions = {}): any {
    const include = options.include ?? [];
    const watchDirs = options.watchDirs ?? ["src/data"];
    let isCompiling = false;

    return {
        name: "cloesce-compile",

        configureServer(server: any) {
            for (const dir of watchDirs) {
                server.watcher.add(dir);
            }
        },

        async hotUpdate({ file, server }: { file: string; server: any }) {
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
            console.log("[cloesce] Running initial compile...");
            try {
                const { stdout, stderr } = await execAsync("npx cloesce compile");
                if (stdout) console.log(stdout);
                if (stderr) console.warn(stderr);
                console.log("[cloesce] Initial compile completed");
            } catch (error: any) {
                console.error(`[cloesce] Initial compile failed: ${error.message}`);
                if (error.stdout) console.error(error.stdout);
                if (error.stderr) console.error(error.stderr);
            }
        },
    };
}

export default cloesce;
