# cloece — TS → JSON Manifest (MVP)

## Goal

Turn a **TypeScript** input into a single **JSON manifest** describing:

* **Entities** → D1 tables + fields
* **CRUD Routes** → Workers
* **Resource list** → D1 + R2 buckets

---

## Scope

* Single database (for now). User defines the DB name.
* **Decorators supported:** `@D1`, `@R2`, `@Workers.GET`, `@Workers.POST`
* **Types supported:** float, int, string, boolean, Date, R2Id
* **Out of scope (v0):** relations, indexes, uniques, defaults, auth, hashing, watch mode, migrations.

---

## Decorators (MVP)

```ts
@D1(options?)              // class: marks an entity
// options: { table?: string }  // optional table override

@R2({ bucket: string })    // field: marks an R2 object key

@Workers.GET(path?)        // method: emits a GET route
@Workers.POST(path?)       // method: emits a POST route
// If path omitted → "/<entity>/<methodName>"
```

---

## Type Mapping

| TS type | IR `type`  | Notes                      |
| ------: | ---------- | -------------------------- |
|  number | int        |                            |
|  string | text       |                            |
| boolean | bool       |                            |
|    Date | date       | store as ISO string        |
|    R2Id | r2\_object | requires `@R2({ bucket })` |

**Nullability:** a field is nullable if it has `?` or is unioned with `null`/`undefined`.
**Primary key rule:** if a numeric field named `id` exists → `pk: true`, `auto: true`. Otherwise: **error** (v0 only supports `id: number` as PK).

---

## JSON Manifest

> **Note**
> Subject to change. Since all of cloece’s functionality depends on the manifest, we may tweak it to fit the Rust compiler’s needs. The POST request syntax is a bit verbose, but we assume users won’t hand-edit this JSON.

```json
{
  "version": "0.0.1",
  "project": "my-app",
  "entities": [
    {
      "name": "Person",
      "table": "person",
      "fields": [
        { "name": "id", "type": "int", "pk": true, "auto": true, "nullable": false },
        { "name": "name", "type": "text", "nullable": false },
        { "name": "age", "type": "int", "nullable": false },
        { "name": "image", "type": "r2_object", "bucket": "images", "nullable": true }
      ]
    }
  ],
  "routes": [
    {
      "method": "GET",
      "path": "/person/foo",
      "handler": "Person.foo"
    },
    {
      "method": "GET",
      "path": "/person/:id/name",
      "handler": "Person.getName",
      "request": {
        "pathParams": {
          "id": { "type": "int", "required": true }
        }
      },
      "response": {
        "returns": {
          "entity": "Person",
          "projection": ["name"]
        },
        "contentType": "application/json",
      }
    }
    {
      "method": "POST",
      "path": "/person",
      "handler": "Person.create",
      "impl": {
        "language": "ts",
        "code": {
          "source": "async function create(db, req, env) {\n  const { name, age, imageKey } = await req.json();\n\n  if (typeof name !== 'string' || typeof age !== 'number') {\n    return new Response(JSON.stringify({ error: 'name (string) and age (number) required' }), { status: 400, headers: { 'content-type': 'application/json' } });\n  }\n  if (age < 0 || age > 150) {\n    return new Response(JSON.stringify({ error: 'age out of range' }), { status: 422, headers: { 'content-type': 'application/json' } });\n  }\n\n  // Optional R2 existence check example (env.R2_IMAGES.head(imageKey))\n\n  const row = await db\n    .prepare('INSERT INTO person (name, age, image) VALUES (?, ?, ?) RETURNING id')\n    .bind(name, age, imageKey ?? null)\n    .first();\n\n  return new Response(JSON.stringify({ id: row?.id, name, age, image: imageKey ?? null }), {\n    status: 201,\n    headers: { 'content-type': 'application/json' }\n  });\n}"
        }
      }
    }
  ],
  "resources": {
    "d1": [{ "name": "default" }],
    "r2": [{ "bucket": "images" }]
  }
}
```

---

## Example (input → output)

### TypeScript Input

```ts
@D1
class Person {
  id: number;             // ← pk: true, auto: true
  name: string;
  age: number;

  @R2({ bucket: "images" })
  image?: R2Id;

  // Simple read path: /person/foo?name=...
  @Workers.GET("/person/foo")
  async foo(db: D1Db, req: Request) {
    const who = new URL(req.url).searchParams.get("name") ?? "world";
    return new Response(JSON.stringify({ hello: who }), {
      headers: { "content-type": "application/json" },
    });
  }

  // Create path: POST /person
  // Body: { "name": string, "age": number, "imageKey"?: string }
  @Workers.POST("/person")
  async create(db: D1Db, req: Request, env: any) {
    const { name, age, imageKey } = await req.json();

    if (typeof name !== "string" || typeof age !== "number") {
      return new Response(
        JSON.stringify({ error: "name (string) and age (number) required" }),
        { status: 400, headers: { "content-type": "application/json" } }
      );
    }
    if (age < 0 || age > 150) {
      return new Response(
        JSON.stringify({ error: "age out of range" }),
        { status: 422, headers: { "content-type": "application/json" } }
      );
    }

    // (Optional) If using R2 you could validate existence:
    // const exists = imageKey && await env.R2_IMAGES.head(imageKey);
    // if (imageKey && !exists) return new Response(JSON.stringify({ error: "image not found" }), { status: 404 });

    const row = await db
      .prepare("INSERT INTO person (name, age, image) VALUES (?, ?, ?) RETURNING id")
      .bind(name, age, imageKey ?? null)
      .first<{ id: number }>();

    return new Response(JSON.stringify({ id: row?.id, name, age, image: imageKey ?? null }), {
      status: 201,
      headers: { "content-type": "application/json" },
    });
  }
}
```

> The generated JSON manifest for this input is shown above in **JSON Manifest**.

---

## Validation

* Classes must have `@D1`.
* Must have `id: number` (exact name `id`) → inferred as `pk: true`, `auto: true`.
* Only the five TS types above are allowed; anything else is an error.
* `@R2` requires a **literal** bucket string and `R2Id` field type.
* Route paths must start with `/` and be **unique** per manifest.
* Decorator args must be **literal-only**.

---

## Naming

* **Table name:** Use `@D1({ table })`.
* **Field names:** use source identifiers as-is.
* **Default route path (when omitted):** `/<entity>/<methodName>`.

---

## CLI (one command)

```
cloece compile [--project <tsconfig path>] [--include <glob>] [--out <file>] [--projectName <name>]
```

---

# Technical Implementation (Brainstorm)

> *This is a working plan. Everything here is subject to change as development progresses.*

## Tech Stack (MVP)

* **Runtime:** Node.js ≥ 18
* **Language:** TypeScript
* **AST tooling:** `ts-morph`
* **File discovery:** `globby`
* **CLI:** `commander`
* **Schema validation:** `zod` (final shape check; primary validation happens during AST walk)
* **Bundling:** `tsup`
* **Publish:** npm

---

## 1) Startup (CLI)

* Parse flags: `--project`, `--include`, `--out`, `--projectName`.
* Resolve files via `globby` and the provided `tsconfig`.

---

## 2) Create the AST

* Load the project with **ts-morph**.
* Add target files and resolve dependencies/diagnostics.

---

## 3) Discover Entities

* For each **class** with `@D1`, mark it as an **entity**.
* Determine the **entity name** and compute the **table name** (default: `snake_case(ClassName)`; allow `@D1({ table })` override).

---

## 4) Extract Fields

* For each **property**, map TypeScript → IR type:

  | TS type   | IR `type`   |
  | --------- | ----------- |
  | `number`  | `int`       |
  | `string`  | `text`      |
  | `boolean` | `bool`      |
  | `Date`    | `date`      |
  | `R2Id`    | `r2_object` |

* Determine **nullability** (`?` or union with `null`/`undefined`).

* If the name is exactly `id` and the type is numeric & non-nullable, mark it as **primary key** (`pk: true`, `auto: true`).

* If decorated with `@R2({ bucket })`, record the **bucket** and ensure the field type is **`R2Id`**.

---

## 5) Extract Routes

* For each **method** with `@Workers.GET(path)` or `@Workers.POST(path)`:

  * Method: `GET` or `POST`.
  * Path: provided literal or default `/<entity>/<methodName>`.
  * Handler: `ClassName.method`.
  * Capture the **method body** and **parameter list** as a single **string** (to embed under `routes[].impl.code.source`).

---

## 6) Accumulate Resources

* **D1:** single item `{ "name": "default" }`.
* **R2:** unique list of buckets discovered on fields with `@R2`.

---

## 7) Build Manifest Object

* Assemble `{ version, project, entities, routes, resources }` in memory.

---

## 8) Validation

* Enforce rules during **AST extraction** (fast, with file\:line diagnostics):

  * Entities have **`id: number`** as the primary key.
  * Only the **allowed field types** (table above).
  * `@R2` uses a **literal** `bucket` and the field type is **`R2Id`**.
  * Routes **start with `/`** and **(method, path)** pairs are unique.
  * **Decorator arguments are literal-only** (no identifiers/expressions).

> *Optionally* run a `zod` shape check on the final in-memory manifest as a guardrail before emitting.

---

## 9) Emit

* Pretty-print the manifest to the `--out` JSON file.
* Exit **non-zero** on any validation/extraction errors with clear, actionable messages.
