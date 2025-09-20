import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { extractModels } from "../src/extract.js";
import { readFileSync } from "fs";

import { readdirSync } from "fs";
import { join } from "path";

test("actions snapshot", () => {
  let models = extractModels({
    version: "0.0.2",
    projectName: "actions",
    cwd: "./tests/fixtures",
    tsconfigPath: "./tests/fixtures/tsconfig.json",
  });

  expect(models).toMatchSnapshot();
});
