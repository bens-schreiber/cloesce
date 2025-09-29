#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { command, run, option, string, optional } from "cmd-ts";
import { CidlExtractor } from "./extract.js";
import { Project } from "ts-morph";

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

function readPackageJsonProjectName(cwd: string) {
  const pkgPath = path.join(cwd, "package.json");
  let projectName = path.basename(cwd);

  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
    projectName = pkg.name ?? projectName;
  }

  return projectName;
}

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

  for (const searchPath of searchPaths) {
    const fullPath = path.resolve(root, searchPath);

    if (!fs.existsSync(fullPath)) {
      console.warn(
        `Warning: Path "${searchPath}" specified in cloesce-config.json does not exist`,
      );
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

run(cli, process.argv.slice(2));
