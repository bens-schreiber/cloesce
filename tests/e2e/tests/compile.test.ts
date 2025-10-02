import path from "path";
import fs from "fs";
import { startWrangler, stopWrangler } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";

const fixturesDir = path.resolve(__dirname, "../../fixtures");

const fixtures = fs.readdirSync(fixturesDir).filter((entry) => {
  const fullPath = path.join(fixturesDir, entry);
  return fs.statSync(fullPath).isDirectory();
});

describe("Check fixture compilation", () => {
  console.log(`Found ${fixtures.length} fixtures: ${fixtures}`);
  fixtures.forEach((fixture) => {
    it(fixture, async () => {
      const fixturePath = path.join(fixturesDir, fixture);
      await startWrangler(fixturePath);
      await stopWrangler();
    });
  });
});
