import fs from "node:fs";
import path from "node:path";
import {
  command,
  run,
  option,
  string,
  optional,
  subcommands,
  boolean,
  flag,
} from "cmd-ts";
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
  handler: async ({ projectName, out, location, truncateSourcePaths }) => {
    await runExtractor({
      projectName,
      out,
      location,
      truncateSourcePaths: truncateSourcePaths,
    });
  },
});

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
  const projectRoot = findProjectRoot(baseDir);
  const outPath = path.resolve(out ?? ".generated/cidl.json");

  const files = findCloesceFiles(projectRoot, ["./"]);
  const project = new Project({
    compilerOptions: {
      strictNullChecks: true,
    },
  });
  files.forEach((f) => project.addSourceFileAtPath(f));

  let cloesceProjectName =
    projectName ?? readPackageJsonProjectName(projectRoot);

  try {
    let extractor = new CidlExtractor(cloesceProjectName, "v0.0.2");
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
    }

    const json = JSON.stringify(result.value, null, 4);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, json);
  } catch (err: any) {
    console.error(
      "Critical uncaught error. Submit a ticket to https://github.com/bens-schreiber/cloesce: ",
      err?.message ?? err
    );
    process.exit(1);
  }
}

function formatErr(e: ExtractorError): string {
  let { description, suggestion } = getErrorInfo(e.code);
  return `
==== CLOESCE ERROR ====
Error [${ExtractorErrorCode[e.code]}]: ${description}
Phase: TypeScript IDL Extraction
Context: ${e.context}

${e.snippet}

Suggested fix: ${suggestion}
  `;
}

function findProjectRoot(start: string) {
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

function findCloesceFiles(root: string, searchPaths: string[]): string[] {
  const files: string[] = [];

  for (const searchPath of searchPaths) {
    const fullPath = path.resolve(root, searchPath);

    if (!fs.existsSync(fullPath)) {
      console.warn(
        `Warning: Path "${searchPath}" specified in cloesce-config.json does not exist`
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
