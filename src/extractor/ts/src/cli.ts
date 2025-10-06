#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { command, run, option, string, optional, flag } from "cmd-ts";
import { CidlExtractor } from "./extract.js";
import { Project } from "ts-morph";
import { ExtractorError, ExtractorErrorCode, getErrorInfo } from "./common.js";

const cli = command({
  name: "cloesce",
  description: "Extract models and write cidl.json",
  args: {
    projectName: option({
      long: "project-name",
      type: optional(string),
      description: "Project name",
      defaultValue: () => "CloesceProject",
    }),
    out: option({
      long: "out",
      short: "o",
      type: optional(string),
      description: "Output path for the CIDL",
      defaultValue: () => ".generated/cidl.json",
    }),
    inp: option({
      long: "in",
      short: "i",
      type: optional(string),
      description: "Input file or directory",
      defaultValue: () => process.cwd(),
    }),
    truncateSourcePaths: flag({
      long: "truncateSourcePaths",
      description:
        "Removes paths from source files, leaving only their file names.",
    }),
  },
  handler: async ({ projectName, out, inp, truncateSourcePaths }) => {
    runExtractor(projectName!, out!, inp!, truncateSourcePaths);
  },
});

function runExtractor(
  projectName: string,
  out: string,
  inp: string,
  truncateSourcePaths: boolean,
) {
  const files = findCloesceFiles(inp, [inp]);
  const project = new Project({
    compilerOptions: {
      strictNullChecks: true,
    },
  });
  files.forEach((f) => project.addSourceFileAtPath(f));

  try {
    let extractor = new CidlExtractor(projectName, "v0.0.2");
    const result = extractor.extract(project);
    if (!result.ok) {
      console.error(formatErr(result.value));
      process.exit(1);
    }
    const ast = result.value;

    if (truncateSourcePaths) {
      ast.wrangler_env.source_path =
        "./" + path.basename(ast.wrangler_env.source_path);

      for (const model of Object.values(ast.models)) {
        model.source_path = "./" + path.basename(model.source_path);
      }

      for (const poo of Object.values(ast.poos)) {
        poo.source_path = "./" + path.basename(poo.source_path);
      }
    }

    const json = JSON.stringify(result.value, null, 4);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, json);
  } catch (err: any) {
    console.error(
      "Critical uncaught error. Submit a ticket to https://github.com/bens-schreiber/cloesce: ",
      err?.message ?? err,
    );
    process.exit(1);
  }
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

run(cli, process.argv.slice(2));
