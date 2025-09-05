#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { command, run, option, string, optional } from "cmd-ts";
import { extractModels } from "./extract.js";

const cli = command({
  name: "cloesce-ts",
  description: "Extract models and write schema.json",
  args: {
    projectName: option({
      long: "project-name",
      short: "p",
      type: optional(string),    
      description: "Project name",
    }),
  },
  handler: async ({ projectName }) => {
    const outPath = path.resolve(process.cwd(), "schema.json");
    try {
      const result = await extractModels({ projectName, cwd: process.cwd() });
      fs.writeFileSync(outPath, JSON.stringify(result, null, 4));
      console.log(`Wrote ${outPath}`);
    } catch (err: any) {
      console.error(" ERROR - cloece-ts:", err?.message ?? err);
      process.exit(1);
    }
  },
});

run(cli, process.argv.slice(2));
