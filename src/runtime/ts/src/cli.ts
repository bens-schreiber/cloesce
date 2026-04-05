#!/usr/bin/env node
import { WASI } from "node:wasi";
import fs from "node:fs";
import { readFile } from "fs/promises";
import path from "node:path";
import {
  command,
  run,
  subcommands,
  flag,
  string,
  positional,
  optional,
} from "cmd-ts";
import { createJiti } from "jiti";
import {
  DefaultCloesceConfig,
  defaultConfig,
  WranglerConfigFormat,
} from "./config.js";

function timestamp(): string {
  const d = new Date();
  return (
    d.getFullYear().toString() +
    String(d.getMonth() + 1).padStart(2, "0") +
    String(d.getDate()).padStart(2, "0") +
    "T" +
    String(d.getHours()).padStart(2, "0") +
    String(d.getMinutes()).padStart(2, "0") +
    String(d.getSeconds()).padStart(2, "0")
  );
}

function debug(...args: any[]) {
  console.log(`\x1b[90m${timestamp()}:\x1b[0m`, ...args);
}
function debugBenchmark(...args: any[]) {
  debug(...args);
  return Date.now();
}

type WasmConfig = {
  name: string;
  args: string[];
  wasmUrl: URL;
  wranglerConfigPath: string;
  env?: Record<string, string>;
};

function findCloesceFiles(searchPaths: string[], root: string): string[] {
  return searchPaths.flatMap((p: string) => {
    const full = path.isAbsolute(p) ? p : path.resolve(root, p);

    if (!fs.existsSync(full)) {
      console.warn(`Warning: Path "${p}" does not exist`);
      return [];
    }

    const stats = fs.statSync(full);

    if (stats.isFile()) {
      return /\.cloesce$/i.test(full) ? [full] : [];
    }

    return collectCloesceFiles(full);
  });
}

function collectCloesceFiles(dir: string): string[] {
  const results: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...collectCloesceFiles(full));
    } else if (entry.isFile() && /\.cloesce$/i.test(entry.name)) {
      results.push(full);
    }
  }
  return results;
}

function wranglerConfigPathFromFormat(format: WranglerConfigFormat): string {
  return format === "jsonc" ? "wrangler.jsonc" : "wrangler.toml";
}

const cmds = subcommands({
  name: "cloesce",
  cmds: {
    compile: command({
      name: "compile",
      description: "Run through the full compilation process.",
      args: {},
      handler: async () => {
        const config = await loadCloesceConfig(process.cwd());
        const wranglerConfigPath = wranglerConfigPathFromFormat(
          config.wranglerConfigFormat,
        );

        const root = process.cwd();
        const compileConfig: WasmConfig = {
          name: "compile",
          wasmUrl: new URL("./compile.wasm", import.meta.url),
          args: [
            config.outPath,
            wranglerConfigPath,
            config.migrationsPath,
            config.workersUrl,
            ...findCloesceFiles(config.srcPaths, root).map((p) =>
              path.relative(root, p),
            ),
          ],
          wranglerConfigPath,
        };

        await runWasm(compileConfig);
      },
    }),
    migrate: command({
      name: "migrate",
      description: "Creates a database migration.",
      args: {
        // When --all is used, only one positional is provided and it lands here (as the name).
        // When --all is not used, this is the D1 binding name and `name` is the migration name.
        binding: positional({
          type: optional(string),
          displayName: "binding",
          description: "The name of the D1 binding to generate a migration for",
        }),
        name: positional({ type: optional(string), displayName: "name" }),
        all: flag({
          long: "all",
          description: "Generate migrations for all D1 bindings",
        }),
        debug: flag({
          long: "debug",
          short: "d",
          description: "Show debug output",
        }),
      },
      handler: async (args) => {
        let bindingArgs: string[];
        let migrationName: string;

        if (args.all) {
          if (!args.binding) {
            console.error(
              "Error: Must provide a migration name. Usage: cloesce migrate --all NAME",
            );
            process.exit(1);
          }
          if (args.name !== undefined) {
            console.error(
              "Error: Unexpected argument. Usage: cloesce migrate --all NAME",
            );
            process.exit(1);
          }
          migrationName = args.binding;
          bindingArgs = ["--all"];
        } else {
          if (!args.binding || !args.name) {
            console.error(
              "Error: Must provide both a binding and a migration name. Usage: cloesce migrate BINDING NAME",
            );
            process.exit(1);
          }
          migrationName = args.name;
          bindingArgs = ["--binding", args.binding];
        }

        const config = await loadCloesceConfig(process.cwd());
        const wranglerConfigPath = wranglerConfigPathFromFormat(
          config.wranglerConfigFormat,
        );

        const cidlPath = path.join(config.outPath, "cidl.json");
        if (!fs.existsSync(cidlPath)) {
          console.error(
            "Err: No cloesce file found, have you ran `cloesce compile`?",
          );
          process.exit(1);
        }

        if (!fs.existsSync(config.migrationsPath)) {
          fs.mkdirSync(config.migrationsPath);
        }

        let wasmArgs = [
          cidlPath,
          ...bindingArgs,
          migrationName,
          wranglerConfigPath,
          ".",
        ];

        const migrateConfig: WasmConfig = {
          name: "migrations",
          wasmUrl: new URL("./migrate.wasm", import.meta.url),
          args: wasmArgs,
          wranglerConfigPath,
        };

        await runWasm(migrateConfig);
      },
    }),
  },
});

async function runWasm(config: WasmConfig) {
  const debugStart = debugBenchmark(`Preparing to run ${config.name} WASM...`);
  const root = process.cwd();

  const wranglerPath = path.join(root, config.wranglerConfigPath);
  if (!fs.existsSync(wranglerPath)) {
    debug(
      `No ${config.wranglerConfigPath} found, creating empty config at ${wranglerPath}.`,
    );
    fs.writeFileSync(wranglerPath, "");
  }
  debug(`Using ${config.wranglerConfigPath} at ${wranglerPath}`);
  const wasi = new WASI({
    version: "preview1",
    args: [config.name, ...config.args],
    env: { ...process.env, ...config.env },
    preopens: { ".": root },
  });

  const readWasmStart = debugBenchmark(`Reading ${config.name} binary...`);
  const wasm = await readFile(config.wasmUrl);
  const mod = await WebAssembly.compile(new Uint8Array(wasm));
  let instance = await WebAssembly.instantiate(mod, {
    wasi_snapshot_preview1: wasi.wasiImport,
  });
  debug(`Read and compiled wasm binary in ${Date.now() - readWasmStart}ms`);
  debug(`Executing WASM with args: ${config.args.join(" ")}`);

  try {
    wasi.start(instance);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`WASM execution failed for ${config.name}: ${msg}`);
  } finally {
    debug(
      `Compilation ${config.name} completed in ${Date.now() - debugStart}ms`,
    );
  }
}

async function loadCloesceConfig(root: string): Promise<DefaultCloesceConfig> {
  // Load cloesce.config.ts
  const configTsPath = path.join(root, "cloesce.config.ts");
  if (fs.existsSync(configTsPath)) {
    try {
      return await _loadCloesceConfig(configTsPath, root);
    } catch (err) {
      console.warn(`Failed to load cloesce.config.ts: ${err}`);
    }
  }

  debug(
    "Using default config since no cloesce.config.ts was found or failed to load.",
  );
  return defaultConfig({
    srcPaths: [],
  });

  async function _loadCloesceConfig(
    configTsPath: string,
    root: string,
  ): Promise<DefaultCloesceConfig> {
    debug(`Attempting to load config from ${configTsPath}`);

    const jitiLoader = createJiti(root, {
      interopDefault: true,
      moduleCache: false,
    });

    const configModule = (await jitiLoader.import(configTsPath)) as {
      default?: any;
    };
    const configBuilder = configModule.default;

    if (!configBuilder || typeof configBuilder.srcPaths === "undefined") {
      throw new Error(
        "cloesce.config.ts must export a config object via export default",
      );
    }

    const config = defaultConfig(configBuilder);
    debug(
      `Cloesce Config: ${JSON.stringify({ srcPaths: config.srcPaths, projectName: (config as any).projectName, outPath: config.outPath, workersUrl: config.workersUrl, migrationsPath: config.migrationsPath, wranglerConfigFormat: config.wranglerConfigFormat, truncateSourcePaths: config.truncateSourcePaths }, null, 2)}`,
    );
    return config;
  }
}

run(cmds, process.argv.slice(2)).catch((err) => {
  console.error(err);
  process.exit(1);
});
