import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { extractModels } from "../src/extract.js";

// ---- helpers ---------------------------------------------------------------

function copyFileToTmp(tmp: string, rel: string, sourcePath: string) {
  const dest = path.join(tmp, rel);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(sourcePath, dest);
  return dest;
}

function writeTmpTsconfig(tmp: string) {
  fs.writeFileSync(
    path.join(tmp, "tsconfig.json"),
    JSON.stringify(
      {
        compilerOptions: {
          target: "ES2022",
          module: "ESNext",
          moduleResolution: "Bundler",
          experimentalDecorators: true,
          skipLibCheck: true,
          strict: true
        },
        include: ["**/*.ts"]
      },
      null,
      2
    )
  );
}

function stubCloesce(tmp: string) {
  fs.mkdirSync(path.join(tmp, "node_modules/cloesce-ts"), { recursive: true });
  fs.writeFileSync(
    path.join(tmp, "node_modules/cloesce-ts/index.d.ts"),
    `
    export declare function D1(...args: any[]): any;
    export declare function PrimaryKey(...args: any[]): any;
    export declare function GET(...args: any[]): any;
    export declare function POST(...args: any[]): any;
    export interface D1Db {}
    export interface Request {}
    export interface Response {}
  `
  );
}

// stable key order for snapshots (so diffs are clean)
function sortDeep<T>(x: T): T {
  if (Array.isArray(x)) return x.map(sortDeep) as any;
  if (x && typeof x === "object") {
    const entries = Object.entries(x as any).sort(([a], [b]) => a.localeCompare(b));
    return Object.fromEntries(entries.map(([k, v]) => [k, sortDeep(v)])) as any;
  }
  return x;
}

// Reusable runner for one file
async function runOnSnapFile(projectName: string, snapRelPath: string) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "cloesce-"));
  writeTmpTsconfig(tmp);
  stubCloesce(tmp);

  // copy the snap file into tmp root (keep same name)
  const sourceFile = path.join(__dirname, "snap", snapRelPath);
  copyFileToTmp(tmp, path.basename(snapRelPath), sourceFile);

  // run extractor on tmp project
  const result = await extractModels({ projectName, cwd: tmp });
  return sortDeep(result);
}

// ---- tests -----------------------------------------------------------------

describe("cloesce-ts extractModels snapshot", () => {
  it("turns person.cloesce.ts into expected JSON spec", async () => {
    const normalized = await runOnSnapFile("Person", "person.cloesce.ts");

    expect(normalized).toMatchInlineSnapshot(`
{
  "language": "typescript",
  "models": [
    {
      "Person": {
        "attributes": [
          {
            "name": "id",
            "nullable": false,
            "pk": true,
            "type": "number",
          },
          {
            "name": "name",
            "nullable": false,
            "type": "string",
          },
          {
            "name": "middle_name",
            "nullable": true,
            "type": "string",
          },
        ],
        "methods": [
          {
            "http_verb": "GET",
            "name": "foo",
            "parameters": [
              {
                "name": "d1",
                "nullable": false,
                "type": "D1Db",
              },
            ],
            "return": {
              "type": "Response",
            },
            "static": false,
          },
          {
            "http_verb": "POST",
            "name": "speak",
            "parameters": [
              {
                "name": "d1",
                "nullable": false,
                "type": "D1Db",
              },
              {
                "name": "phrase",
                "nullable": false,
                "type": "string",
              },
            ],
            "return": {
              "type": "Response",
            },
            "static": true,
          },
        ],
      },
    },
  ],
  "project_name": "Person",
  "version": "0.0.1",
}
`);
  });

  it("turns dog.cloesce.ts into expected JSON spec", async () => {
    const normalized = await runOnSnapFile("dog", "dog.cloesce.ts");

    expect(normalized).toMatchInlineSnapshot(`
{
  "language": "typescript",
  "models": [
    {
      "Dog": {
        "attributes": [
          {
            "name": "id",
            "nullable": false,
            "pk": true,
            "type": "number",
          },
          {
            "name": "name",
            "nullable": false,
            "type": "string",
          },
          {
            "name": "breed",
            "nullable": false,
            "type": "number",
          },
          {
            "name": "preferred_treat",
            "nullable": true,
            "type": "string",
          },
        ],
        "methods": [
          {
            "http_verb": "GET",
            "name": "get_name",
            "parameters": [
              {
                "name": "d1",
                "nullable": false,
                "type": "D1Db",
              },
            ],
            "return": {
              "type": "Response",
            },
            "static": false,
          },
          {
            "http_verb": "GET",
            "name": "get_breed",
            "parameters": [
              {
                "name": "d1",
                "nullable": false,
                "type": "D1Db",
              },
            ],
            "return": {
              "type": "Response",
            },
            "static": false,
          },
          {
            "http_verb": "POST",
            "name": "woof",
            "parameters": [
              {
                "name": "d1",
                "nullable": false,
                "type": "D1Db",
              },
              {
                "name": "phrase",
                "nullable": false,
                "type": "string",
              },
            ],
            "return": {
              "type": "Response",
            },
            "static": true,
          },
        ],
      },
    },
  ],
  "project_name": "dog",
  "version": "0.0.1",
}
`);
  });
});
