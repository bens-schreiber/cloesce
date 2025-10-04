<<<<<<< Updated upstream
#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { command, run, option, string, optional } from "cmd-ts";
import { CidlExtractor } from "./extract.js";
=======
// ts/src/cli.ts
import { WASI } from "node:wasi";
import fs from "node:fs";
import path from "node:path";
import {
  command,
  run,
  subcommands,
  flag,
  option,
  optional,
  string,
  positional,
} from "cmd-ts";
>>>>>>> Stashed changes
import { Project } from "ts-morph";
import { CidlExtractor } from "./extract.js";

<<<<<<< Updated upstream
const cli = command({
  name: "cloesce",
  description: "Extract models and write cidl.json",
  args: {
    projectName: option({
      long: "project-name",
      short: "p",
      type: optional(string),
      description: "Project name",
    }),
    out: option({
      long: "out",
      short: "o",
      type: optional(string),
      description: "Output path (default: <project>/.generated/cidl.json)",
    }),
  },
  handler: async ({ projectName, out }) => {
    const projectRoot = findProjectRoot(process.cwd());
    const outPath = path.resolve(projectRoot, out ?? ".generated/cidl.json");
    fs.mkdirSync(path.dirname(outPath), { recursive: true });

    // Collect cloesce files
    const config = readCloesceConfig(projectRoot);
    const sourcePaths = Array.isArray(config.source)
      ? config.source
      : [config.source];
    const files = findCloesceFiles(projectRoot, sourcePaths);
    if (files.length === 0) {
      throw new Error(
        `No ".cloesce.ts" files found in specified source path(s): ${sourcePaths.join(", ")}`,
      );
    }

    // Setup TypeScript project for AST traversal
    const tsconfigPath = fs.existsSync(path.join(projectRoot, "tsconfig.json"))
      ? path.join(projectRoot, "tsconfig.json")
      : undefined;
    const project = new Project({
      tsConfigFilePath: tsconfigPath,
      compilerOptions: tsconfigPath
        ? undefined
        : { target: 99, lib: ["es2022", "dom"] },
    });
    files.forEach((f) => project.addSourceFileAtPath(f));

    // Read pkg.json to set the cloesce project name
    let cloesceProjectName =
      projectName ?? readPackageJsonProjectName(projectRoot);

    try {
      let extractor = new CidlExtractor(cloesceProjectName, "v0.0.2");
      const result = extractor.extract(project);
      if (!result.ok) {
        throw new Error(result.value);
      }

      fs.writeFileSync(outPath, JSON.stringify(result.value, null, 4));
      console.log(`Wrote ${outPath}`);
    } catch (err: any) {
      console.error(" ERROR - cloesce:", err?.message ?? err);
      process.exit(1);
    }
  },
});

function findProjectRoot(start: string) {
  // optional: ensure output lands at the project root, not a random cwd
  let dir = start;
  for (;;) {
    if (fs.existsSync(path.join(dir, "package.json"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return start;
    dir = parent;
  }
}
=======
/* ===================== WASM CONFIGURATIONS ===================== */
type WasmConfig = {
  name: string;
  description?: string;
  wasmPath: string;
  preopenDir: string;
  args: string[];
  env?: Record<string, string>;
};

const WASM_CONFIGS: WasmConfig[] = [
  {
    name: "wrangler",
    description: "Generate wrangler.toml configuration",
    wasmPath:
      "C:\\code-projects\\cloesce\\src\\generator\\target\\wasm32-wasip1\\release\\cli.wasm",
    preopenDir: "C:\\code-projects\\cloesce",
    args: ["generate", "wrangler", "src/generated/wrangler.toml"],
  },
  {
    name: "schema",
    description: "Generate database schema",
    wasmPath:
      "C:\\code-projects\\cloesce\\src\\generator\\target\\wasm32-wasip1\\release\\cli.wasm",
    preopenDir: "C:\\code-projects\\cloesce",
    args: [
      "generate",
      "d1",
      "src/generated/cidl.json",
      "src/generated/migrations.sql",
    ],
  },
  {
    name: "workers",
    description: "None",
    wasmPath:
      "C:\\code-projects\\cloesce\\src\\generator\\target\\wasm32-wasip1\\release\\cli.wasm",
    preopenDir: "C:\\code-projects\\cloesce",
    args: [
      "generate",
      "workers",
      "src/generated/cidl.json",
      "src/generated/workers.ts",
      "src/generated/wrangler.toml",
      "http://localhost:5002/api",
    ],
  },
  {
    name: "client",
    description: "None",
    wasmPath:
      "C:\\code-projects\\cloesce\\src\\generator\\target\\wasm32-wasip1\\release\\cli.wasm",
    preopenDir: "C:\\code-projects\\cloesce",
    args: [
      "generate",
      "client",
      "src/generated/cidl.json",
      "src/generated/client.ts",
      "http://localhost:5002/api",
    ],
  }
];

/* ===================== CIDL EXTRACT (unchanged) ===================== */
function findPackageJsonRoot(baseDir: string) {
  let dir = baseDir;
  for (;;) {
    if (fs.existsSync(path.join(dir, "package.json"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return baseDir;
    dir = parent;
  }
}
function outDir(root: string) {
  return path.join(root, "generated");
}
function readPackageJsonProjectName(cwd: string) {
  const pkgPath = path.join(cwd, "package.json");
  let projectName = path.basename(cwd);
  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
    projectName = pkg.name ?? projectName;
  }
  return projectName;
}
function findCloesceFiles(root: string, searchPaths: string[]): string[] {
  const files: string[] = [];
  for (const searchPath of searchPaths) {
    const fullPath = path.resolve(root, searchPath);
    if (!fs.existsSync(fullPath)) continue;
    const stats = fs.statSync(fullPath);
    if (stats.isFile() && /\.cloesce\.ts$/i.test(fullPath)) {
      files.push(fullPath);
    } else if (stats.isDirectory()) {
      files.push(...walkDirectory(fullPath));
    }
  }
  return files;

  function walkDirectory(dir: string): string[] {
    const found: string[] = [];
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        found.push(...walkDirectory(fullPath));
      } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
        found.push(fullPath);
      }
    }
    return found;
  }
}
async function ensureDirClean(dir: string) {
  await fs.promises.rm(dir, { recursive: true, force: true });
  await fs.promises.mkdir(dir, { recursive: true });
}
async function runExtractor({
  projectName,
  out,
  location,
  truncateSourcePaths,
}: {
  projectName?: string;
  out?: string;
  location?: string;
  truncateSourcePaths?: boolean;
}) {
  const baseDir = location ? path.resolve(location) : process.cwd();
  const projectRoot = findPackageJsonRoot(baseDir);
  const genDir = outDir(projectRoot);
  const outPath = path.resolve(out ?? path.join(genDir, "cidl.json"));

  const files = findCloesceFiles(projectRoot, ["./"]);
  const project = new Project({ compilerOptions: { strictNullChecks: true } });
  files.forEach((f) => project.addSourceFileAtPath(f));

  const name = projectName ?? readPackageJsonProjectName(projectRoot);

  try {
    await ensureDirClean(genDir);
    const extractor = new CidlExtractor(name, "v0.0.2");
    const result = extractor.extract(project);
    if (!result.ok) throw new Error(result.value);
    const ast = result.value;

    if (truncateSourcePaths) {
      ast.wrangler_env.source_path =
        "./" + path.basename(ast.wrangler_env.source_path);
      for (const model of Object.values(ast.models)) {
        // @ts-ignore dynamic shape
        model.source_path = "./" + path.basename(model.source_path);
      }
    }

    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(ast, null, 4));
    console.log(`✅ Wrote CIDL to ${outPath}`);
  } catch (err: any) {
    console.error("ERROR - cidl extract:", err?.message ?? err);
    process.exit(1);
  }
}

/* ===================== FILE HANDLING ===================== */
async function ensureOutputDirectory(mountRoot: string, filePath: string) {
  const fullPath = path.join(mountRoot, filePath);
  const dir = path.dirname(fullPath);
>>>>>>> Stashed changes

  // Ensure the directory exists
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
    console.log(`📁 Created directory: ${dir}`);
  }

  return fullPath;
}

<<<<<<< Updated upstream
type CloesceConfig = {
  source: string | string[];
};

function readCloesceConfig(cwd: string): CloesceConfig {
  const configPath = path.join(cwd, "cloesce-config.json");

  if (!fs.existsSync(configPath)) {
    throw new Error(
      `No "cloesce-config.json" found in "${cwd}". Please create a cloesce-config.json with a "source" field.`,
    );
  }

  try {
    const config = JSON.parse(
      fs.readFileSync(configPath, "utf8"),
    ) as CloesceConfig;

    if (!config.source) {
      throw new Error('cloesce-config.json must contain a "source" field');
    }

    return config;
  } catch (error) {
    throw new Error(
      `Failed to parse cloesce-config.json: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function findCloesceFiles(root: string, searchPaths: string[]): string[] {
  const files: string[] = [];
=======
/* ===================== WASM EXECUTION ===================== */
async function runWasmCommand(config: WasmConfig) {
  const wasmPath = path.resolve(config.wasmPath);
  if (!fs.existsSync(wasmPath)) {
    throw new Error(`WASM not found at: ${wasmPath}`);
  }
  const mountRoot = path.resolve(config.preopenDir);
>>>>>>> Stashed changes

  // Ensure output directories exist for all output files in args
  for (const arg of config.args) {
    // Check if this looks like an output file path (contains extension and doesn't start with -)
    if (arg.includes(".") && !arg.startsWith("-")) {
      await ensureOutputDirectory(mountRoot, arg);
    }
  }

<<<<<<< Updated upstream
  return files;
}

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
=======
  const wasi = new WASI({
    version: "preview1",
    args: [path.basename(wasmPath), ...config.args],
    env: { ...process.env, ...config.env } as Record<string, string>,
    preopens: { "/": mountRoot },
  });

  const bytes = fs.readFileSync(wasmPath);
  const mod = await WebAssembly.compile(bytes);
  const instance = await WebAssembly.instantiate(mod, {
    wasi_snapshot_preview1: wasi.wasiImport,
  });

  console.log(`\n🚀 Running WASM command: ${config.name}`);
  console.log(`📍 WASM Path: ${wasmPath}`);
  console.log(`📂 Mount Root: ${mountRoot}`);
  console.log(`⚙️  Arguments: ${config.args.join(" ")}`);
  console.log(`${"─".repeat(50)}`);

  wasi.start(instance);

  console.log(`${"─".repeat(50)}`);
  console.log(`✅ WASM command '${config.name}' completed successfully\n`);
}

async function listWasmCommands() {
  console.log("\n📋 Available WASM commands:\n");
  console.log(`${"─".repeat(60)}`);

  for (const config of WASM_CONFIGS) {
    console.log(
      `  ${config.name.padEnd(15)} ${config.description || "No description"}`,
    );
  }

  console.log(`${"─".repeat(60)}`);
  console.log("\nUsage: cloesce wasm <command-name>");
  console.log("       cloesce wasm --all (run all commands)\n");
}

/* ===================== CLI COMMANDS ===================== */
const extractCmd = command({
  name: "extract",
  description: "Extract models and write generated/cidl.json",
  args: {
    projectName: option({ long: "project-name", type: optional(string) }),
    out: option({
      long: "out",
      type: optional(string),
      description: "Output path (default: <root>/generated/cidl.json)",
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
  handler: async ({ projectName, out, location, truncateSourcePaths }) =>
    runExtractor({ projectName, out, location, truncateSourcePaths }),
});

const wasmCmd = command({
  name: "wasm",
  description: "Run WASM commands configured in the CLI",
  args: {
    commandName: positional({
      type: optional(string),
      displayName: "command",
      description: "Name of the WASM command to run (or use --list to see all)",
    }),
    list: flag({
      long: "list",
      short: "l",
      description: "List all available WASM commands",
    }),
    all: flag({
      long: "all",
      description: "Run all configured WASM commands in sequence",
    }),
  },
  handler: async ({ commandName, list, all }) => {
    // Handle --list flag
    if (list) {
      await listWasmCommands();
      return;
    }

    // Handle --all flag
    if (all) {
      console.log("\n🔄 Running all WASM commands...\n");
      for (const config of WASM_CONFIGS) {
        try {
          await runWasmCommand(config);
        } catch (err: any) {
          console.error(`❌ Failed to run '${config.name}':`, err.message);
          process.exit(1);
        }
      }
      console.log("✨ All WASM commands completed successfully!");
      return;
    }

    // Handle specific command
    if (!commandName) {
      console.error(
        "❌ Error: Please specify a command name or use --list to see available commands",
      );
      await listWasmCommands();
      process.exit(1);
    }
>>>>>>> Stashed changes

    const config = WASM_CONFIGS.find((c) => c.name === commandName);
    if (!config) {
      console.error(`❌ Error: Unknown WASM command '${commandName}'`);
      await listWasmCommands();
      process.exit(1);
    }

    try {
      await runWasmCommand(config);
    } catch (err: any) {
      console.error(`❌ Failed to run '${commandName}':`, err.message);
      process.exit(1);
    }
  },
});

const router = subcommands({
  name: "cloesce",
  cmds: { extract: extractCmd, wasm: wasmCmd },
});

(async () => {
  await run(router, process.argv.slice(2));
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
