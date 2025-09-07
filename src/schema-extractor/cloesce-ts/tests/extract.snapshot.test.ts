import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { extractModels } from "../src/extract.js";

// ── Embedded class configurations ─────────────────────────────────────────────
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

const ACTIONS_CLASS = `import { D1, D1Db, POST, PUT, PATCH, DELETE, PrimaryKey } from "cloesce-ts";

@D1
class Actions {
  @PrimaryKey id!: number;
  name!: string;

  @POST
  static create(db: D1Db, req: Request, payload: string) {
    return new Response("ok", { status: 201 });
  }

  @PUT
  static update(db: D1Db, req: Request, id: number, payload: string | null) {
    return new Response("ok");
  }

  @PATCH
  static patchlol(db: D1Db, req: Request, phrase: string) {
    return new Response("ok");
  }

  @DELETE
  static remove(db: D1Db, req: Request, id: number) {
    return new Response("ok");
  }
}
`;

// ── Helpers ──────────────────────────────────────────────────────────────────
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
          strict: true,
          lib: ["ES2022", "DOM"],
        },
        include: ["**/*.ts"],
      },
      null,
      2,
    ),
  );
}

function writeConfig(tmp: string, source: string = ".") {
  fs.writeFileSync(
    path.join(tmp, "cloesce-config.json"),
    JSON.stringify({ source }, null, 2),
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
    export declare function PUT(...args: any[]): any;
    export declare function PATCH(...args: any[]): any;
    export declare function DELETE(...args: any[]): any;
    export interface D1Db {}
    // Use the DOM ones at runtime; these are just to satisfy the type checker if needed.
    // If you prefer, you can omit these and rely on "lib": ["DOM"] above.
    // export interface Request {}
    // export interface Response {}
  `,
  );
}

// stable order for snapshots / comparisons
function sortDeep<T>(x: T): T {
  if (Array.isArray(x)) return x.map(sortDeep) as any;
  if (x && typeof x === "object") {
    const entries = Object.entries(x as any).sort(([a], [b]) =>
      a.localeCompare(b),
    );
    return Object.fromEntries(entries.map(([k, v]) => [k, sortDeep(v)])) as any;
  }
  return x;
}

async function runOnClassContent(
  projectName: string,
  fileName: string,
  classContent: string,
) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "cloesce-"));
  writeTmpTsconfig(tmp);
  stubCloesce(tmp);

  fs.writeFileSync(path.join(tmp, fileName), classContent);
  writeConfig(tmp, "."); // make sure extractor sees our *.cloesce.ts

  const result = await extractModels({ projectName, cwd: tmp });
  return sortDeep(result);
}

// ── Tests ────────────────────────────────────────────────────────────────────
describe("cloesce-ts extractModels", () => {
  it("turns person.cloesce.ts into expected JSON spec (wrapped attributes, cidl_type, skip db/request)", async () => {
    const res = await runOnClassContent(
      "Person",
      "person.cloesce.ts",
      PERSON_CLASS,
    );
    expect(res.language).toBe("TypeScript");

    const person = res.models.find((m: any) => m.name === "Person");
    expect(person).toBeTruthy();

    // Attribute wrapping: { value: { name, cidl_type, nullable }, primary_key }
    expect(person.attributes).toEqual(
      expect.arrayContaining([
        {
          value: { name: "id", cidl_type: "Integer", nullable: false },
          primary_key: true,
        },
        {
          value: { name: "name", cidl_type: "Text", nullable: false },
          primary_key: false,
        },
        {
          value: { name: "middle_name", cidl_type: "Text", nullable: true },
          primary_key: false,
        },
      ]),
    );

    // Methods: POST static, GET instance; params should skip db & Request
    const post = person.methods.find((m: any) => m.name === "speak");
    expect(post).toMatchObject({
      name: "speak",
      is_static: true,
      http_verb: "POST",
    });
    expect(post.parameters).toEqual([
      { name: "phrase", cidl_type: "Text", nullable: false },
    ]);

    const get = person.methods.find((m: any) => m.name === "foo");
    expect(get).toMatchObject({
      name: "foo",
      is_static: false,
      http_verb: "GET",
    });
    expect(get.parameters).toEqual([]); // db and Request omitted
  });

  it("turns dog.cloesce.ts into expected JSON spec", async () => {
    const res = await runOnClassContent("dog", "dog.cloesce.ts", DOG_CLASS);
    expect(res.language).toBe("TypeScript");

    const dog = res.models.find((m: any) => m.name === "Dog");
    expect(dog).toBeTruthy();

    // basic attribute checks
    expect(dog.attributes).toEqual(
      expect.arrayContaining([
        {
          value: { name: "id", cidl_type: "Integer", nullable: false },
          primary_key: true,
        },
        {
          value: { name: "name", cidl_type: "Text", nullable: false },
          primary_key: false,
        },
        {
          value: { name: "breed", cidl_type: "Integer", nullable: false },
          primary_key: false,
        },
        {
          value: { name: "preferred_treat", cidl_type: "Text", nullable: true },
          primary_key: false,
        },
      ]),
    );

    // GET methods should have no params (db/request skipped)
    const gets = dog.methods.filter((m: any) => m.http_verb === "GET");
    expect(gets).toHaveLength(2);
    gets.forEach((m: any) => {
      expect(m.is_static).toBe(false);
      expect(m.parameters).toEqual([]);
    });

    // static POST woof should only include phrase
    const woof = dog.methods.find((m: any) => m.name === "woof");
    expect(woof).toMatchObject({ http_verb: "POST", is_static: true });
    expect(woof.parameters).toEqual([
      { name: "phrase", cidl_type: "Text", nullable: false },
    ]);
  });

  it("captures ALL static HTTP verbs (POST/PUT/PATCH/DELETE) and skips db/request params", async () => {
    const res = await runOnClassContent(
      "verbs",
      "verbs.cloesce.ts",
      ACTIONS_CLASS,
    );
    expect(res.language).toBe("TypeScript");

    const actions = res.models.find((m: any) => m.name === "Actions");
    expect(actions).toBeTruthy();

    // Make a small map for convenience
    const byName: Record<string, any> = Object.fromEntries(
      actions.methods.map((m: any) => [m.name, m]),
    );

    expect(byName.create).toMatchObject({
      name: "create",
      http_verb: "POST",
      is_static: true,
    });
    expect(byName.create.parameters).toEqual([
      { name: "payload", cidl_type: "Text", nullable: false },
    ]);

    expect(byName.update).toMatchObject({
      name: "update",
      http_verb: "PUT",
      is_static: true,
    });
    expect(byName.update.parameters).toEqual([
      { name: "id", cidl_type: "Integer", nullable: false },
      { name: "payload", cidl_type: "Text", nullable: true }, // string | null
    ]);

    expect(byName.patchlol).toMatchObject({
      name: "patchlol",
      http_verb: "PATCH",
      is_static: true,
    });
    expect(byName.patchlol.parameters).toEqual([
      { name: "phrase", cidl_type: "Text", nullable: false },
    ]);

    expect(byName.remove).toMatchObject({
      name: "remove",
      http_verb: "DELETE",
      is_static: true,
    });
    expect(byName.remove.parameters).toEqual([
      { name: "id", cidl_type: "Integer", nullable: false },
    ]);

    // Sanity: ensure no param leaked as D1Db / db / Request
    actions.methods.forEach((m: any) => {
      m.parameters.forEach((p: any) => {
        expect(p.name).not.toBe("db");
        expect(p.cidl_type).not.toBe("D1Db");
      });
    });
  });
});
