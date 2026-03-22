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
  option,
  optional,
} from "cmd-ts";
import { createJiti } from "jiti";
import { CidlExtractor } from "./extractor/extract.js";
import {
  ExtractorError,
  ExtractorErrorCode,
  getErrorInfo,
} from "./extractor/err.js";
import { CloesceAst } from "./ast.js";
import {
  CloesceConfigBuilder,
  DefaultCloesceConfig,
  WranglerConfigFormat,
} from "./config.js";
import { Project } from "ts-morph";

let debugPhase: "extractor" | "npm cloesce" = "npm cloesce";
function debug(...args: any[]) {
  console.log(`${timestamp()} [${debugPhase}]: `, ...args);
}
function debugBenchmark(...args: any[]) {
  debug(...args);
  return Date.now();
}

type WasmConfig = {
  name: string;
  description?: string;
  wasmFile: string;
  args: string[];
  wranglerConfigPath: string;
  env?: Record<string, string>;
};

function wranglerConfigPathFor(format: WranglerConfigFormat): string {
  return format === "jsonc" ? "wrangler.jsonc" : "wrangler.toml";
}

function toWasiPath(filePath: string, root: string): string {
  const normalize = (value: string) => value.replace(/\\/g, "/");
  const normalizedRoot = normalize(path.resolve(root));
  const normalizedFile = normalize(path.resolve(root, filePath));

  const rootKey =
    process.platform === "win32"
      ? normalizedRoot.toLowerCase()
      : normalizedRoot;
  const fileKey =
    process.platform === "win32"
      ? normalizedFile.toLowerCase()
      : normalizedFile;

  if (fileKey === rootKey) {
    return ".";
  }

  if (fileKey.startsWith(`${rootKey}/`)) {
    return normalizedFile.slice(normalizedRoot.length + 1);
  }

  return normalize(filePath);
}

const cmds = subcommands({
  name: "cloesce",
  cmds: {
    compile: command({
      name: "compile",
      description: "Run through the full compilation process.",
      args: {},
      handler: async () => {
        const root = process.cwd();
        const config = await loadCloesceConfig(process.cwd());
        const wranglerConfigPath = wranglerConfigPathFor(
          config.wranglerConfigFormat,
        );

        await extract(config);
        debugPhase = "npm cloesce";

        const outputDir = config.outPath;
        const generateConfig: WasmConfig = {
          name: "generate",
          wasmFile: "generator.wasm",
          args: [
            "generate",
            toWasiPath(path.join(outputDir, "cidl.pre.json"), root),
            toWasiPath(path.join(outputDir, "cidl.json"), root),
            toWasiPath(wranglerConfigPath, root),
            toWasiPath(path.join(outputDir, "workers.ts"), root),
            toWasiPath(path.join(outputDir, "client.ts"), root),
            config.workersUrl,
            config.migrationsPath,
          ],
          wranglerConfigPath,
        };

        await generate(generateConfig);
      },
    }),
    extract: command({
      name: "extract",
      description: "Extract models and write cidl.pre.json",
      args: {
        projectName: option({
          long: "project-name",
          type: optional(string),
          description: "Project name",
        }),
        out: option({
          long: "out",
          short: "o",
          type: optional(string),
        }),
        inp: option({
          long: "in",
          short: "i",
          type: optional(string),
          description: "Input file or directory",
        }),
        truncateSourcePaths: flag({
          long: "truncateSourcePaths",
          description: "Sets all source paths to just their file name",
        }),
      },
      handler: async (args) => {
        const config = new CloesceConfigBuilder({
          projectName: args.projectName,
          outPath: args.out,
          srcPaths: [args.inp!],
          truncateSourcePaths: args.truncateSourcePaths,
        });

        await extract(config);
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
        const root = process.cwd();
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
        const wranglerConfigPath = wranglerConfigPathFor(
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
          "migrations",
          toWasiPath(cidlPath, root),
          ...bindingArgs,
          migrationName,
          toWasiPath(wranglerConfigPath, root),
          toWasiPath(".", root),
        ];

        const migrateConfig: WasmConfig = {
          name: "migrations",
          wasmFile: "generator.wasm",
          args: wasmArgs,
          wranglerConfigPath,
        };

        // Runs a generator command. Exits the process on failure.
        await generate(migrateConfig);
      },
    }),
  },
});

async function extract(
  config: DefaultCloesceConfig,
  args: {
    truncateSourcePaths?: boolean;
  } = {},
) {
  debugPhase = "extractor";
  const startTime = debugBenchmark("Preparing extraction...");

  const root = process.cwd();
  const projectRoot = process.cwd();

  const searchPaths = config.srcPaths;

  const outPath = (() => {
    // If the out path is a directory, join it with "cidl.pre.json"
    if (
      fs.existsSync(config.outPath) &&
      fs.statSync(config.outPath).isDirectory()
    ) {
      return path.join(config.outPath, "cidl.pre.json");
    }

    // If the out path is a file, use it directly
    if (config.outPath.endsWith(".json")) {
      return config.outPath;
    }

    // Default to .generated/cidl.pre.json
    return path.join(config.outPath, "cidl.pre.json");
  })();

  const truncate =
    args.truncateSourcePaths ?? config.truncateSourcePaths ?? false;
  const cloesceProjectName =
    config.projectName ?? readPackageJsonProjectName(projectRoot);

  const project = new Project({
    compilerOptions: {
      skipLibCheck: true,
      experimentalDecorators: true,
      emitDecoratorMetadata: true,
      strict: true,
    },
  });

  project.addSourceFilesAtPaths(
    searchPaths.flatMap((p: string) => {
      const full = path.isAbsolute(p) ? p : path.resolve(root, p);

      if (!fs.existsSync(full)) {
        console.warn(`Warning: Path "${p}" does not exist`);
        return [];
      }

      const stats = fs.statSync(full);

      if (stats.isFile()) {
        return /\.cloesce\.ts$/i.test(full) ? [full] : [];
      }

      return [path.join(full, "**/*.cloesce.ts")];
    }),
  );

  const fileCount = project.getSourceFiles().length;
  if (fileCount === 0) {
    console.warn("No .cloesce.ts files found in the specified paths.");
    process.exit(1);
  }
  debug(`Found ${fileCount} .cloesce.ts files`);

  try {
    const extractorStart = debugBenchmark("Extracting CIDL...");

    const result = CidlExtractor.extract(cloesceProjectName, project);
    if (result.isLeft()) {
      console.error(formatErr(result.value));
      process.exit(1);
    }

    let ast: CloesceAst = result.unwrap();

    if (truncate) {
      if (ast.wrangler_env) {
        ast.wrangler_env.source_path =
          "./" + path.basename(ast.wrangler_env.source_path);
      }

      if (ast.main_source) {
        ast.main_source = "./" + path.basename(ast.main_source);
      }

      for (const model of Object.values(ast.models)) {
        model.source_path = "./" + path.basename(model.source_path);
      }

      for (const poo of Object.values(ast.poos)) {
        poo.source_path = "./" + path.basename(poo.source_path);
      }

      for (const service of Object.values(ast.services)) {
        service.source_path = "./" + path.basename(service.source_path);
      }
    }

    // Run all AST modifiers from the config on the extracted AST
    for (const modifier of config.astModifiers) {
      modifier(ast);
    }

    const json = JSON.stringify(ast, null, 4);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, json);

    debug(
      `Successfully extracted cidl.pre.json ${outPath} in ${Date.now() - extractorStart}ms`,
    );
    return { outPath, projectName: cloesceProjectName };
  } catch (err: any) {
    console.error(
      "Critical uncaught error in extractor. \nSubmit a ticket to https://github.com/bens-schreiber/cloesce\n\n",
      err?.message ?? "No error message.",
      "\n",
      err?.stack ?? "No error stack.",
    );
    process.exit(1);
  } finally {
    debug(`Extraction process completed in ${Date.now() - startTime}ms`);
  }
}

async function generate(config: WasmConfig) {
  const debugStart = debugBenchmark(`Starting generator`);
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
    args: ["generate", ...config.args],
    env: { ...process.env, ...config.env },
    preopens: { ".": root },
  });

  const readWasmStart = debugBenchmark(`Reading generator binary...`);
  const wasm = await readFile(new URL("./generator.wasm", import.meta.url));
  const mod = await WebAssembly.compile(new Uint8Array(wasm));
  let instance = await WebAssembly.instantiate(mod, {
    wasi_snapshot_preview1: wasi.wasiImport,
  });
  debug(
    `Read and compiled generator wasm binary in ${Date.now() - readWasmStart}ms`,
  );

  try {
    wasi.start(instance);
  } catch (err) {
    console.error(`WASM execution failed for ${config.name}:`, err);
    throw err;
  } finally {
    debug(`Generator ${config.name} completed in ${Date.now() - debugStart}ms`);
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
  return CloesceConfigBuilder.fromDefault();

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

    debug(
      `Cloesce Config: ${JSON.stringify({ srcPaths: configBuilder.srcPaths, projectName: configBuilder.projectName, outPath: configBuilder.outPath, workersUrl: configBuilder.workersUrl, migrationsPath: configBuilder.migrationsPath, wranglerConfigFormat: configBuilder.wranglerConfigFormat, truncateSourcePaths: configBuilder.truncateSourcePaths }, null, 2)}`,
    );
    return configBuilder;
  }
}

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

function readPackageJsonProjectName(cwd: string): string {
  const pkgPath = path.join(cwd, "package.json");
  let projectName = path.basename(cwd);

  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
    projectName = pkg.name ?? projectName;
  }

  return projectName;
}

function formatErr(e: ExtractorError): string {
  const { description, suggestion } = getErrorInfo(e.code);

  const contextLine = e.context ? `Context: ${e.context}\n` : "";
  const snippetLine = e.snippet ? `${e.snippet}\n\n` : "";

  return `
==== CLOESCE ERROR ====
Error [${ExtractorErrorCode[e.code]}]: ${description}
Phase: TypeScript IDL Extraction
${contextLine}${snippetLine}Suggested fix: ${suggestion}

`;
}

run(cmds, process.argv.slice(2)).catch((err) => {
  console.error(err);
  process.exit(1);
});
