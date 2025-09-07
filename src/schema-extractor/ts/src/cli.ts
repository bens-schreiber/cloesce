#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { command, run, option, string, optional } from "cmd-ts";
import { extractModels } from "./extract.js";

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
    try {
      fs.mkdirSync(path.dirname(outPath), { recursive: true }); // <-- create folders
      const result = await extractModels({ projectName, cwd: projectRoot });
      fs.writeFileSync(outPath, JSON.stringify(result, null, 4));
      console.log(`Wrote ${outPath}`);
    } catch (err: any) {
      console.error(" ERROR - cloesce:", err?.message ?? err);
      process.exit(1);
    }
  },
});

run(cli, process.argv.slice(2));
