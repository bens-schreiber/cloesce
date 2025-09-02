#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { extractModels } from "./extract.js";

const args = process.argv.slice(2);
function getProjectName(): string | undefined {
  const i = args.findIndex(a => a === "--project-name" || a.startsWith("--project-name="));
  if (i === -1) return undefined;
  const eq = args[i].indexOf("=");
  return eq !== -1 ? args[i].slice(eq + 1) : args[i + 1];
}

const projectName = getProjectName();
const outPath = path.resolve(process.cwd(), "schema.json");

try {
  const result = extractModels({ projectName, cwd: process.cwd() });
  fs.writeFileSync(outPath, JSON.stringify(result, null, 4));
  console.log(`Wrote ${outPath}`);
} catch (err: any) {
  console.error("❌ cloece-ts:", err?.message ?? err);
  process.exit(1);
}
