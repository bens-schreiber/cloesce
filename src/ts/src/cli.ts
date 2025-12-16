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
import { Project } from "ts-morph";
import { CidlExtractor } from "./extractor/extract.js";
import {
  ExtractorError,
  ExtractorErrorCode,
  getErrorInfo,
} from "./extractor/err.js";

let debugPhase: "extractor" | "npm cloesce" = "npm cloesce";
function debug(...args: any[]) {
  console.log(`[${debugPhase}]: `, ...args);
}

type WasmConfig = {
  name: string;
  description?: string;
  wasmFile: string;
  args: string[];
  env?: Record<string, string>;
};

type CloesceConfig = {
  paths?: string[];
  projectName?: string;
  truncateSourcePaths?: boolean;
  outputDir?: string;
  workersUrl?: string;
  migrationsPath?: string;
};

const cmds = subcommands({
  name: "cloesce",
  cmds: {
    compile: command({
      name: "compile",
      description: "Run through the full compilation process.",
      args: {},
      handler: async () => {
        const config = loadCloesceConfig(process.cwd());

        if (!config.workersUrl) {
          console.error(
            "Error: `workersUrl`` must be defined in cloesce.config.json",
          );
          process.exit(1);
        }

        await extract(config);
        debugPhase = "npm cloesce";

        const outputDir = config.outputDir ?? ".generated";
        const generateConfig: WasmConfig = {
          name: "generate",
          wasmFile: "generator.wasm",
          args: [
            "generate",
            path.join(outputDir, "cidl.pre.json"),
            path.join(outputDir, "cidl.json"),
            "wrangler.toml",
            path.join(outputDir, "workers.ts"),
            path.join(outputDir, "client.ts"),
            config.workersUrl,
          ],
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
        location: option({
          long: "location",
          short: "l",
          type: optional(string),
          description: "Project directory (default: cwd)",
        }),
        truncateSourcePaths: flag({
          long: "truncateSourcePaths",
          description: "Sets all source paths to just their file name",
        }),
        debug: flag({
          long: "debug",
          short: "d",
          description: "Show debug output",
        }),
        skipTsCheck: flag({
          long: "skipTsCheck",
          description: "Skip TypeScript compilation checks",
        }),
      },
      handler: async (args) => {
        await extract({ ...args });
      },
    }),

    migrate: command({
      name: "migrate",
      description: "Creates a database migration.",
      args: {
        name: positional({ type: string, displayName: "name" }),
        debug: flag({
          long: "debug",
          short: "d",
          description: "Show debug output",
        }),
      },
      handler: async (args) => {
        const config = loadCloesceConfig(process.cwd());

        const cidlPath = path.join(
          config.outputDir ?? ".generated",
          "cidl.json",
        );
        if (!fs.existsSync(cidlPath)) {
          console.error(
            "Err: No cloesce file found, have you ran `cloesce compile`?",
          );
          process.exit(1);
        }

        const migrationsPath = "./migrations";
        if (!fs.existsSync(migrationsPath)) {
          fs.mkdirSync(migrationsPath);
        }

        const migrationPrefix = path.join(
          migrationsPath,
          `${timestamp()}_${args.name}`,
        );
        let wasmArgs = [
          "migrations",
          cidlPath,
          `${migrationPrefix}.json`,
          `${migrationPrefix}.sql`,
        ];

        // Add last migration if exists
        {
          const files = fs.readdirSync(migrationsPath);
          const jsonFiles = files.filter((f) => f.endsWith(".json"));

          // Sort descending by filename
          jsonFiles.sort((a, b) =>
            b.localeCompare(a, undefined, { numeric: true }),
          );

          if (jsonFiles.length > 0) {
            wasmArgs.push(path.join(migrationsPath, jsonFiles[0]));
          }
        }

        const migrateConfig: WasmConfig = {
          name: "migrations",
          wasmFile: "generator.wasm",
          args: wasmArgs,
        };

        // Runs a generator command. Exits the process on failure.
        await generate(migrateConfig);
      },
    }),
  },
});

async function extract(
  config: CloesceConfig,
  args: {
    projectName?: string;
    out?: string;
    inp?: string;
    truncateSourcePaths?: boolean;
    skipTsCheck?: boolean;
  } = {},
) {
  const startTime = Date.now();
  debugPhase = "extractor";
  debug("Preparing extraction...");

  const root = process.cwd();
  const projectRoot = process.cwd();

  const searchPaths = args.inp ? [args.inp] : (config.paths ?? [root]);
  const outPath =
    args.out ?? path.join(config.outputDir ?? ".generated", "cidl.pre.json");
  const truncate =
    args.truncateSourcePaths ?? config.truncateSourcePaths ?? false;
  const cloesceProjectName =
    args.projectName ??
    config.projectName ??
    readPackageJsonProjectName(projectRoot);

  const project = new Project({
    skipAddingFilesFromTsConfig: true,
    compilerOptions: {
      skipLibCheck: true,
      strictNullChecks: true,
      experimentalDecorators: true,
      emitDecoratorMetadata: true,
    },
  });
  findCloesceProject(root, searchPaths, project);

  const fileCount = project.getSourceFiles().length;
  if (fileCount === 0) {
    new ExtractorError(ExtractorErrorCode.MissingFile);
  }
  debug(`Found ${fileCount} .cloesce.ts files`);

  // Run typescript compiler checks to before extraction
  if (!args.skipTsCheck) {
    const tscStart = Date.now();
    debug("Running TypeScript compiler checks...");

    const diagnostics = project.getPreEmitDiagnostics();
    if (diagnostics.length > 0) {
      console.error("TypeScript errors detected in provided files:");
      console.error(project.formatDiagnosticsWithColorAndContext(diagnostics));
      process.exit(1);
    }

    debug(`TypeScript checks completed in ${Date.now() - tscStart}ms`);
  }

  try {
    const extractorStart = Date.now();
    debug("Extracting CIDL...");

    const extractor = new CidlExtractor(cloesceProjectName, "v0.0.4");
    const result = extractor.extract(project);
    if (result.isLeft()) {
      console.error(formatErr(result.value));
      process.exit(1);
    }

    let ast = result.unwrap();

    if (truncate) {
      if (ast.wrangler_env) {
        ast.wrangler_env.source_path =
          "./" + path.basename(ast.wrangler_env.source_path);
      }

      if (ast.app_source) {
        ast.app_source = "./" + path.basename(ast.app_source);
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
  const debugStart = Date.now();
  debug(`Starting generator`);

  const root = process.cwd();

  // Look for wrangler.toml in the root directory
  const wranglerPath = path.join(root, "wrangler.toml");
  if (!fs.existsSync(wranglerPath)) {
    debug("No wrangler.toml found, creating empty file.");
    fs.writeFileSync(wranglerPath, "");
  }
  debug(`Using wrangler.toml at ${wranglerPath}`);

  const wasi = new WASI({
    version: "preview1",
    args: ["generate", ...config.args],
    env: { ...process.env, ...config.env } as Record<string, string>,
    preopens: { ".": root },
  });

  const readWasmStart = Date.now();
  debug(`Reading generator binary...`);

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

function loadCloesceConfig(root: string): CloesceConfig {
  const configPath = path.join(root, "cloesce.config.json");
  if (!fs.existsSync(configPath)) {
    debug("No cloesce.config.json found, using default");
    return {};
  }

  try {
    const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
    debug(`Loaded config from ${configPath}`);
    return config;
  } catch (err) {
    console.warn(`Failed to parse cloesce.config.json: ${err}`);
    throw err;
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

function findCloesceProject(
  root: string,
  searchPaths: string[],
  project: Project,
): void {
  for (const searchPath of searchPaths) {
    let fullPath: string;

    if (path.isAbsolute(searchPath) || searchPath.startsWith(root)) {
      fullPath = path.normalize(searchPath);
    } else {
      fullPath = path.resolve(root, searchPath);
    }

    if (!fs.existsSync(fullPath)) {
      console.warn(`Warning: Path "${searchPath}" does not exist`);
      continue;
    }

    const stats = fs.statSync(fullPath);
    if (stats.isFile() && /\.cloesce\.ts$/i.test(fullPath)) {
      debug(`Found file: ${fullPath}`);

      project.addSourceFileAtPath(fullPath);
    } else if (stats.isDirectory()) {
      debug(`Searching directory: ${fullPath}`);
      walkDirectory(fullPath);
    }
  }

  function walkDirectory(dir: string): void {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory() && !entry.name.startsWith(".")) {
        debug(`Entering directory: ${fullPath}`);
        walkDirectory(fullPath);
      } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
        debug(`Found file: ${fullPath}`);
        project.addSourceFileAtPath(fullPath);
      }
    }
  }
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
