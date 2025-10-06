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
  positional,
} from "cmd-ts";
import { Project } from "ts-morph";
import { CidlExtractor } from "./extract.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/* ===================== WASM CONFIGURATIONS ===================== */
type WasmConfig = {
  name: string;
  description?: string;
  wasmFile: string;
  args: string[];
  env?: Record<string, string>;
};

// Wasm binary should be bundled with the package
const WASM_PATH = path.join(__dirname, "..", "wasm", "cli.wasm");

const WASM_CONFIGS: WasmConfig[] = [
  {
    name: "wrangler",
    description: "Generate wrangler.toml configuration",
    wasmFile: "cli.wasm",
    args: ["generate", "wrangler", "generated/wrangler.toml"],
  },
  {
    name: "schema",
    description: "Generate database schema",
    wasmFile: "cli.wasm",
    args: ["generate", "d1", "generated/cidl.json", "generated/migrations.sql"],
  },
  {
    name: "workers",
    description: "Generate workers TypeScript",
    wasmFile: "cli.wasm",
    args: [
      "generate",
      "workers",
      "generated/cidl.json",
      "generated/workers.ts",
      "generated/wrangler.toml",
      "http://localhost:5002/api",
    ],
  },
  {
    name: "client",
    description: "Generate client TypeScript",
    wasmFile: "cli.wasm",
    args: [
      "generate",
      "client",
      "generated/cidl.json",
      "generated/client.ts",
      "http://localhost:5002/api",
    ],
  }
];

/* ===================== UTILITIES ===================== */
function findPackageRoot(dir: string = process.cwd()): string {
  let current = path.resolve(dir);
  while (current !== path.parse(current).root) {
    if (fs.existsSync(path.join(current, "package.json"))) {
      return current;
    }
    current = path.dirname(current);
  }
  return path.resolve(dir);
}

function getProjectName(root: string): string {
  const pkgPath = path.join(root, "package.json");
  if (fs.existsSync(pkgPath)) {
    try {
      const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
      if (pkg.name) return pkg.name;
    } catch {}
  }
  return path.basename(root);
}

function findCloesceFiles(root: string): string[] {
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory() && !entry.name.startsWith(".") && entry.name !== "node_modules") {
        walk(fullPath);
      } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
        files.push(fullPath);
      }
    }
  };
  walk(root);
  return files;
}

/* ===================== CIDL EXTRACT ===================== */
async function runExtractor(opts: {
  projectName?: string;
  out?: string;
  location?: string;
  truncateSourcePaths?: boolean;
}) {
  const root = findPackageRoot(opts.location);
  const genDir = path.join(root, "generated");
  const outPath = path.resolve(opts.out ?? path.join(genDir, "cidl.json"));

  const files = findCloesceFiles(root);
  if (files.length === 0) {
    throw new Error("No .cloesce.ts files found in project");
  }

  const project = new Project({ compilerOptions: { strictNullChecks: true } });
  files.forEach(f => project.addSourceFileAtPath(f));

  const name = opts.projectName ?? getProjectName(root);

  try {
    // Only create cidl.json, don't clean entire directory
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    
    const extractor = new CidlExtractor(name, "v0.0.2");
    const result = extractor.extract(project);
    if (!result.ok) throw new Error(result.value);
    
    const ast = result.value;
    if (opts.truncateSourcePaths) {
      ast.wrangler_env.source_path = "./" + path.basename(ast.wrangler_env.source_path);
      for (const model of Object.values(ast.models)) {
        (model as any).source_path = "./" + path.basename((model as any).source_path);
      }
    }

    fs.writeFileSync(outPath, JSON.stringify(ast, null, 4));
    console.log(`✅ Wrote CIDL to ${outPath}`);
  } catch (err: any) {
    console.error("ERROR - cidl extract:", err?.message ?? err);
    process.exit(1);
  }
}

/* ===================== WASM EXECUTION ===================== */
async function runWasmCommand(config: WasmConfig) {
  const root = findPackageRoot();

  if (!fs.existsSync(WASM_PATH)) {
    throw new Error(`WASM file not found. Expected at: ${WASM_PATH}`);
  }

  // Ensure output directories exist
  for (const arg of config.args) {
    if (arg.startsWith("generated/") && arg.includes(".")) {
      const fullPath = path.join(root, arg);
      fs.mkdirSync(path.dirname(fullPath), { recursive: true });
    }
  }

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
  wasi.start(instance);
  console.log(`✅ Completed: ${config.name}\n`);
}

/* ===================== CLI COMMANDS ===================== */
const extractCmd = command({
  name: "extract",
  description: "Extract models and write generated/cidl.json",
  args: {
    projectName: option({ long: "project-name", type: optional(string) }),
    out: option({ long: "out", type: optional(string) }),
    truncateSourcePaths: flag({ long: "truncateSourcePaths" }),
    location: option({ long: "location", short: "l", type: optional(string) }),
  },
  handler: runExtractor,
});

const wasmCmd = command({
  name: "wasm",
  description: "Run WASM code generation",
  args: {
    commandName: positional({ type: optional(string), displayName: "command" }),
    list: flag({ long: "list", short: "l" }),
    all: flag({ long: "all" }),
  },
  handler: async ({ commandName, list, all }) => {
    if (list) {
      console.log("\n📋 Available commands:\n");
      for (const config of WASM_CONFIGS) {
        console.log(`  ${config.name.padEnd(15)} ${config.description || ""}`);
      }
      console.log("\nUsage: cloesce wasm <command>");
      return;
    }

    if (all) {
      for (const config of WASM_CONFIGS) {
        await runWasmCommand(config);
      }
      return;
    }

    if (!commandName) {
      console.error("Specify a command or use --list");
      process.exit(1);
    }

    const config = WASM_CONFIGS.find(c => c.name === commandName);
    if (!config) {
      console.error(`Unknown command: ${commandName}`);
      process.exit(1);
    }

    await runWasmCommand(config);
  },
});

const router = subcommands({
  name: "cloesce",
  cmds: { extract: extractCmd, wasm: wasmCmd },
});

run(router, process.argv.slice(2)).catch(err => {
  console.error(err);
  process.exit(1);
});