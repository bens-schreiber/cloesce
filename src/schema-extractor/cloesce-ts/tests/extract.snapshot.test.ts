import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { extractModels } from "../src/extract.js";

// Embedded class configurations as strings
const PERSON_CLASS = `import { D1, D1Db, GET, POST, PrimaryKey } from "cloesce-ts";

@D1
class Person {
  @PrimaryKey
  id!: number;
  name!: string;
  middle_name: string | null;
  
  @GET
  async foo(db: D1Db, req: Request) {
    const who = new URL(req.url).searchParams.get("name") ?? "world";
    return new Response(JSON.stringify({ hello: who }), {
      headers: { "content-type": "application/json" },
    });
  }
  
  @POST
  static async speak(db: D1Db, req: Request, phrase: string) {
    return new Response(JSON.stringify({ phrase }), {
      status: 201,
      headers: { "content-type": "application/json" },
    });
  }
}`;

const DOG_CLASS = `import { D1, D1Db, GET, POST, PrimaryKey } from "cloesce-ts";

@D1
class Dog {
  @PrimaryKey
  id!: number;
  name!: string;
  breed!: number;
  preferred_treat: string | null;
  
  @GET
  async get_name(db: D1Db, req: Request) {
    const who = new URL(req.url).searchParams.get("name");
    return new Response(JSON.stringify({ hello: who }), {
      headers: { "content-type": "application/json" },
    });
  }
  
  @GET
  async get_breed(db: D1Db, req: Request) {
    const breed = new URL(req.url).searchParams.get("breed");
    return new Response(JSON.stringify({ hello: breed }), {
      headers: { "content-type": "application/json" },
    });
  }
  
  @POST
  static async woof(db: D1Db, req: Request, phrase: string) {
    return new Response(JSON.stringify({ phrase }), {
      status: 201,
      headers: { "content-type": "application/json" },
    });
  }
}`;

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

// stable order for snapshots 
function sortDeep<T>(x: T): T {
  if (Array.isArray(x)) return x.map(sortDeep) as any;
  if (x && typeof x === "object") {
    const entries = Object.entries(x as any).sort(([a], [b]) => a.localeCompare(b));
    return Object.fromEntries(entries.map(([k, v]) => [k, sortDeep(v)])) as any;
  }
  return x;
}

// Modified runner that takes class content directly
async function runOnClassContent(projectName: string, fileName: string, classContent: string) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "cloesce-"));
  writeTmpTsconfig(tmp);
  stubCloesce(tmp);

  // Write the class content directly to a .cloesce.ts file
  fs.writeFileSync(path.join(tmp, fileName), classContent);

  // run extractor on tmp project
  const result = await extractModels({ projectName, cwd: tmp });
  return sortDeep(result);
}

// tests here!
describe("cloesce-ts extractModels snapshot", () => {
  it("turns person.cloesce.ts into expected JSON spec", async () => {
    const normalized = await runOnClassContent("Person", "person.cloesce.ts", PERSON_CLASS);

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
            "is_static": false,
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
          },
          {
            "http_verb": "POST",
            "is_static": true,
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
    const normalized = await runOnClassContent("dog", "dog.cloesce.ts", DOG_CLASS);

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
            "is_static": false,
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
          },
          {
            "http_verb": "GET",
            "is_static": false,
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
          },
          {
            "http_verb": "POST",
            "is_static": true,
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