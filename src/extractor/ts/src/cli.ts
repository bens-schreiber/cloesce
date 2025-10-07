#!/usr/bin/env node
import { WASI } from "node:wasi";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  command,
  run,
  subcommands,
  flag,
  option,
  optional,
  string,
} from "cmd-ts";
import { Project } from "ts-morph";
import { CidlExtractor } from "./extract.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

type WasmConfig = {
  name: string;
  description?: string;
  wasmFile: string;
  args: string[];
  env?: Record<string, string>;
};

// Wasm binary should be bundled with the package
const WASM_PATH = path.join(__dirname, "..", "wasm", "cli.wasm");

function getWasmConfigs(workersUrl: string, clientUrl: string): WasmConfig[] {
  return [
    {
      name: "wrangler",
      description: "Generate wrangler.toml configuration",
      wasmFile: "cli.wasm",
      args: ["generate", "wrangler", ".generated/wrangler.toml"],
    },
    {
      name: "schema",
      description: "Generate database schema",
      wasmFile: "cli.wasm",
      args: [
        "generate",
        "d1",
        ".generated/cidl.json",
        ".generated/migrations.sql",
      ],
    },
    {
      name: "workers",
      description: "Generate workers TypeScript",
      wasmFile: "cli.wasm",
      args: [
        "generate",
        "workers",
        ".generated/cidl.json",
        ".generated/workers.ts",
        ".generated/wrangler.toml",
        workersUrl,
      ],
    },
    {
      name: "client",
      description: "Generate client TypeScript",
      wasmFile: "cli.wasm",
      args: [
        "generate",
        "client",
        ".generated/cidl.json",
        ".generated/client.ts",
        clientUrl,
      ],
    },
  ];
}

type CloesceConfig = {
  paths?: string[];
  projectName?: string;
  truncateSourcePaths?: boolean;
  outputDir?: string;
};

function loadCloesceConfig(root: string): CloesceConfig {
  const configPath = path.join(root, "cloesce-config.json");
  if (fs.existsSync(configPath)) {
    try {
      const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
      console.log(`Loaded config from ${configPath}`);
      return config;
    } catch (err) {
      console.warn(`⚠️ Failed to parse cloesce-config.json: ${err}`);
    }
  }
  return {};
}

function formatErr(e: ExtractorError): string {
  const { description, suggestion } = getErrorInfo(e.code);

  const contextLine = e.context ? `Context: ${e.context}\n` : "";
  const snippetLine = e.snippet ? `${e.snippet}\n\n` : "";

  return `==== CLOESCE ERROR ====
Error [${ExtractorErrorCode[e.code]}]: ${description}
Phase: TypeScript IDL Extraction
${contextLine}${snippetLine}Suggested fix: ${suggestion}`;
}

function findCloesceFiles(root: string, searchPaths: string[]): string[] {
  const files: string[] = [];

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
      files.push(fullPath);
    } else if (stats.isDirectory()) {
      files.push(...walkDirectory(fullPath));
    }
  }

  return files;

  function walkDirectory(dir: string): string[] {
    const files: string[] = [];

    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);

      if (entry.isDirectory()) {
        files.push(...walkDirectory(fullPath));
      } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
        files.push(fullPath);
      }
    }

    return files;
  }
}

async function runExtractor(opts: {
  projectName?: string;
  out?: string;
  location?: string;
  truncateSourcePaths?: boolean;
  silent?: boolean;
}) {
  const baseDir = opts.location ? path.resolve(opts.location) : process.cwd();
  const projectRoot = findProjectRoot(baseDir);
  const config = loadCloesceConfig(projectRoot);

  // Merge config with CLI options
  const searchPaths = config.paths ?? ["./"];
  const outputDir = config.outputDir ?? ".generated";
  const outPath = path.resolve(opts.out ?? path.join(outputDir, "cidl.json"));
  const truncate =
    opts.truncateSourcePaths ?? config.truncateSourcePaths ?? false;
  const cloesceProjectName =
    opts.projectName ??
    config.projectName ??
    readPackageJsonProjectName(projectRoot);

  const files = findCloesceFiles(projectRoot, searchPaths);
  if (files.length === 0) {
    throw new Error(
      `No .cloesce.ts files found in specified paths: ${searchPaths.join(", ")}`,
    );
  }

  if (!opts.silent) {
    console.log(`🔍 Found ${files.length} .cloesce.ts files`);
  }

  const project = new Project({
    compilerOptions: {
      strictNullChecks: true,
    },
  });
  files.forEach((f) => project.addSourceFileAtPath(f));

  try {
    // Clean the entire generated directory to ensure fresh output
    const genDir = path.dirname(outPath);
    if (fs.existsSync(genDir)) {
      const filesToClean = [
        "cidl.json",
        "wrangler.toml",
        "workers.ts",
        "client.ts",
        "migrations.sql",
      ];
      for (const file of filesToClean) {
        const filePath = path.join(genDir, file);
        if (fs.existsSync(filePath)) {
          fs.unlinkSync(filePath);
        }
      }
    }
    fs.mkdirSync(genDir, { recursive: true });

    const extractor = new CidlExtractor(cloesceProjectName, "v0.0.3");
    const result = extractor.extract(project);

    if (!result.ok) {
      process.exit(1);
    }

    let ast = result.value;

    // Fix models structure - convert array to object if needed
    if (Array.isArray(ast.models)) {
      const modelsObj: any = {};
      for (const model of ast.models) {
        if (model.name) {
          modelsObj[model.name] = model;
        }
      }
      ast.models = modelsObj;
    }

    // Fix poos structure - convert array to object if needed
    if ((ast as any).poos && Array.isArray((ast as any).poos)) {
      const poosObj: any = {};
      for (const poo of (ast as any).poos) {
        if (poo.name) {
          poosObj[poo.name] = poo;
        }
      }
      (ast as any).poos = poosObj;
    }

    if (truncate) {
      ast.wrangler_env.source_path =
        "./" + path.basename(ast.wrangler_env.source_path);

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
    fs.writeFileSync(outPath, json);

    if (!opts.silent) {
      console.log(`✅ Wrote CIDL to ${outPath}`);
    }

    return outPath;
  } catch (err: any) {
    console.error(
      "Critical uncaught error. Submit a ticket to https://github.com/bens-schreiber/cloesce: ",
      err?.message ?? err,
    );
    process.exit(1);
  }
}

// wasm execution
async function runWasmCommand(
  config: WasmConfig,
  skipExtract: boolean = false,
) {
  const root = findProjectRoot(process.cwd());
  const outputDir = ".generated";

  // Validate CIDL was created successfully
  const cidlPath = path.join(root, outputDir, "cidl.json");
  if (!fs.existsSync(cidlPath)) {
    throw new Error(
      `CIDL file not found at ${cidlPath}. Extraction may have failed.`,
    );
  }

  if (!fs.existsSync(WASM_PATH)) {
    throw new Error(`WASM file not found. Expected at: ${WASM_PATH}`);
  }

  // Ensure output directory exists
  fs.mkdirSync(path.join(root, outputDir), { recursive: true });

  const wasi = new WASI({
    version: "preview1",
    args: [path.basename(WASM_PATH), ...config.args],
    env: { ...process.env, ...config.env } as Record<string, string>,
    preopens: { "/": root },
  });

  const bytes = fs.readFileSync(WASM_PATH);
  const mod = await WebAssembly.compile(bytes);
  const instance = await WebAssembly.instantiate(mod, {
    wasi_snapshot_preview1: wasi.wasiImport,
  });

  console.log(`🚀 Running: ${config.name}`);

  try {
    wasi.start(instance);
    console.log(`✅ Completed: ${config.name}\n`);
  } catch (err) {
    console.error(`❌ WASM execution failed for ${config.name}:`, err);
  }
}

// Main run command that does everything
const runCmd = command({
  name: "run",
  description:
    "Extract CIDL and run all code generators (requires --workers and --client URLs)",
  args: {
    workers: option({
      long: "workers",
      type: string,
      description: "Workers URL (e.g., http://localhost:5002/api)",
    }),
    client: option({
      long: "client",
      type: string,
      description: "Client URL (e.g., http://localhost:50002/api)",
    }),
  },
  handler: async (args) => {
    console.log("🚀 Running complete generation pipeline...\n");
    console.log(`   Workers URL: ${args.workers}`);
    console.log(`   Client URL: ${args.client}\n`);

    await runExtractor({ silent: false });

    const configs = getWasmConfigs(args.workers, args.client);

    for (const config of configs) {
      await runWasmCommand(config, true);
    }

    console.log("🎉 Generation complete!");
  },
});

// Keep extract as a separate command if we just want to run the CIDL extractor
const extractCmd = command({
  name: "extract",
  description: "Extract models and write cidl.json only",
  args: {
    projectName: option({
      long: "project-name",
      type: optional(string),
      description: "Project name",
    }),
    out: option({
      long: "out",
      type: optional(string),
      description: "Output path (default: <project>/.generated/cidl.json)",
    }),
    truncateSourcePaths: flag({
      long: "truncateSourcePaths",
      description: "Sets all source paths to just their file name",
    }),
    location: option({
      long: "location",
      short: "l",
      type: optional(string),
      description: "Project directory (default: cwd)",
    }),
  },
  handler: async (args) => {
    await runExtractor({ ...args, silent: false });
  },
});

const router = subcommands({
  name: "cloesce",
  cmds: {
    run: runCmd,
    extract: extractCmd,
  },
});

run(router, process.argv.slice(2)).catch((err) => {
  console.error(err);
  process.exit(1);
});
