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
import { ExtractorError, ExtractorErrorCode, getErrorInfo } from "./common.js";

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
  clientUrl?: string;
  migrationsPath?: string;
};

const cmds = subcommands({
  name: "cloesce",
  cmds: {
    compile: command({
      name: "compile",
      description: "Run through the full compilation process.",
      args: {
        debug: flag({
          long: "debug",
          short: "d",
          description: "Show debug output",
        }),
      },
      handler: async (args) => {
        const config = loadCloesceConfig(process.cwd(), args.debug);

        if (!config.workersUrl || !config.clientUrl) {
          console.error(
            "Error: `workersUrl` and `clientUrl` must be defined in cloesce.config.json",
          );
          process.exit(1);
        }

        // Creates a `cidl.json` file. Exits the process on failure.
        await extract({ debug: args.debug });

        const outputDir = config.outputDir ?? ".generated";

        const allConfig: WasmConfig = {
          name: "all",
          wasmFile: "generator.wasm",
          args: [
            "generate",
            "all",
            path.join(outputDir, "cidl.pre.json"),
            path.join(outputDir, "cidl.json"),
            "wrangler.toml",
            path.join(outputDir, "workers.ts"),
            path.join(outputDir, "client.ts"),
            config.clientUrl,
            config.workersUrl,
          ],
        };

        // Runs a generator command. Exits the process on failure.
        await generate(allConfig);
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
        const config = loadCloesceConfig(process.cwd(), args.debug);

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

async function extract(opts: {
  projectName?: string;
  out?: string;
  inp?: string;
  truncateSourcePaths?: boolean;
  debug?: boolean;
}) {
  const root = process.cwd();
  const projectRoot = process.cwd();
  const config = loadCloesceConfig(projectRoot, opts.debug);

  const searchPaths = opts.inp ? [opts.inp] : (config.paths ?? [root]);
  const outputDir = config.outputDir ?? ".generated";
  const outPath = opts.out ?? path.join(outputDir, "cidl.pre.json");
  const truncate =
    opts.truncateSourcePaths ?? config.truncateSourcePaths ?? false;
  const cloesceProjectName =
    opts.projectName ??
    config.projectName ??
    readPackageJsonProjectName(projectRoot);

  const project = new Project({
    compilerOptions: {
      strictNullChecks: true,
    },
  });

  findCloesceProject(root, searchPaths, project);

  const fileCount = project.getSourceFiles().length;
  if (fileCount === 0) {
    new ExtractorError(ExtractorErrorCode.MissingFile);
  }

  if (opts.debug) console.log(`Found ${fileCount} .cloesce.ts files`);

  try {
    const extractor = new CidlExtractor(cloesceProjectName, "v0.0.3");
    const result = extractor.extract(project);

    if (!result.ok) {
      console.error(formatErr(result.value));
      process.exit(1);
    }

    let ast = result.value;

    if (truncate) {
      ast.wrangler_env.source_path =
        "./" + path.basename(ast.wrangler_env.source_path);

      if (ast.app_source) {
        ast.app_source = "./" + path.basename(ast.app_source);
      }

      for (const model of Object.values(ast.models)) {
        (model as any).source_path =
          "./" + path.basename((model as any).source_path);
      }

      if ((ast as any).poos) {
        for (const poo of Object.values((ast as any).poos)) {
          (poo as any).source_path =
            "./" + path.basename((poo as any).source_path);
        }
      }
    }

    const json = JSON.stringify(ast, null, 4);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, json);

    console.log(`CIDL extracted to ${outPath}`);

    return { outPath, projectName: cloesceProjectName };
  } catch (err: any) {
    console.error(
      "Critical uncaught error in generator. \nSubmit a ticket to https://github.com/bens-schreiber/cloesce\n\n",
      err?.message ?? "No error message.",
      "\n",
      err?.stack ?? "No error stack.",
    );
    process.exit(1);
  }
}

async function generate(config: WasmConfig) {
  const root = process.cwd();

  // Look for wrangler.toml in the root directory
  const wranglerPath = path.join(root, "wrangler.toml");
  if (!fs.existsSync(wranglerPath)) {
    fs.writeFileSync(wranglerPath, "");
  }

  const wasi = new WASI({
    version: "preview1",
    args: ["generate", ...config.args],
    env: { ...process.env, ...config.env } as Record<string, string>,
    preopens: { ".": root },
  });

  const wasm = await readFile(new URL("./generator.wasm", import.meta.url));
  const mod = await WebAssembly.compile(new Uint8Array(wasm));
  let instance = await WebAssembly.instantiate(mod, {
    wasi_snapshot_preview1: wasi.wasiImport,
  });

  try {
    wasi.start(instance);
  } catch (err) {
    console.error(`WASM execution failed for ${config.name}:`, err);
  }
}

function loadCloesceConfig(
  root: string,
  debug: boolean = false,
): CloesceConfig {
  const configPath = path.join(root, "cloesce.config.json");
  if (fs.existsSync(configPath)) {
    try {
      const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
      if (debug) console.log(`Loaded config from ${configPath}`);
      return config;
    } catch (err) {
      console.warn(`Failed to parse cloesce.config.json: ${err}`);
    }
  }
  return {};
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
      project.addSourceFileAtPath(fullPath);
    } else if (stats.isDirectory()) {
      walkDirectory(fullPath, project);
    }
  }

  function walkDirectory(dir: string, project: Project): void {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory() && !entry.name.startsWith(".")) {
        walkDirectory(fullPath, project);
      } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
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
