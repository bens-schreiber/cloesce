# Cloece — TS → JSON Manifest (MVP)

## Goal

Turn a **TypeScript** input into a single **JSON manifest** describing:

* **Entities** → D1 tables + fields
* **CRUD Routes** → Workers
* **Resource list** → D1

> *To do this we essentially have to create a domain specific compiler.*

---

## Scope

* Single database (for now). User defines the DB name.
* **Decorators supported:** `@D1`, `@GET`, `@POST`
* **Types supported:** float, int, string, boolean, Date
* **Out of scope (v0.0.1):** relations, indexes, uniques, defaults, auth, hashing, watch mode, migrations.

---

## Decorators (MVP)

```ts
@D1(options?)              // class: marks an entity
// options: { table?: string }  // optional table override

@GET(path?)        // method: emits a GET route
@POST(path?)       // method: emits a POST route
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

**Nullability:** a field is nullable if it has ` | null` attatched to it.
**Primary key rule:** primary key must explicitly be defined using `@PrimaryKey` decorator.

---

## JSON Manifest

> **Note**
> Subject to change. Since all of Cloece’s functionality depends on the manifest, we may tweak it to fit the code generators needs.

```json
{
    "version": "0.0.1",
    "project_name": "..",
    "language": "typescript",
    "models": [
        {
            "Person": {
                "attributes": [
                    {
                        "name": "id",
                        "type": 0,
                        "nullable": false,
                        "pk": true
                    },
                    {
                        "name": "name",
                        "type:": 1,
                        "nullable": false
                    },
                    {
                        "name": "middle_name",
                        "type:": 1,
                        "nullable": true
                    },
                ],
                "methods": [
                    {
                        "name": "speak",
                        "static": true,
                        "http_verb": "POST",
                        "parameters": [
                            {
                                "name": "phrase",
                                "type": 1, // string
                                "nullable": false
                            },
                            {
                                "name": "d1",
                                "type": 6 // d1 type
                            }
                        ],
                        "return": {
                            "type": 7 // json with http result
                        }
                    },
                    {
                        "name": "foo",
                        "static": false,
                        "http_verb": "GET",
                        "parameters": [
                            {
                                "name": "d1",
                                "type": 6 // d1 type
                            }
                        ],
                        "return": {
                            "type": 7 // json with http result
                        }
                    }
                ]
            }
        }
    ]
}
```

---

## Example (input → output)

### TypeScript Input

```ts
@D1
class Person {
  @PrimaryKey
  id: number;
  name: string;
  middle_name: string | null

  @GET
  async foo(db: D1Db, req: Request) {
    const who = new URL(req.url).searchParams.get("name") ?? "world";
    return new Response(JSON.stringify({ hello: who }), {
      headers: { "content-type": "application/json" },
    });
  }

  // path override
  @POST("/person/speak")
  static async speak(db: D1Db, req: Request, phrase: string) {
    return new Response(JSON.stringify({ phrase }), {
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
* Must have `id: number` (exact name `id`) and explicit declaration of primary key.
* Only the five TS types above are allowed; anything else is an error.
* Route paths, when overriden, must start with `/` and be **unique** per manifest.
* Decorator args must be **literal-only**.
* For methods, the return type must be a Response.

---

## Naming

* **Table name:** Use `@D1({ table })`.
* **Field names:** use source identifiers as-is.
* **Default route path (when omitted):** `/<entity>/<methodName>`.

---

## CLI (one command)

```
cloece-ts compile [--project <tsconfig path>] [--include <glob>] [--projectName <name>]
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
* **Schema validation:** `zod` (Will use for unit testing)
* **Bundling:** `tsup`
* **Publish:** npm

---

## 1) Startup (CLI)

* Parse flags: `--project`, `--include`, `--projectName`.
* Resolve files via `globby` and the provided `tsconfig`.

---

## 2) Create the AST

* Load the project with **ts-morph**.
* Add target files and resolve dependencies/diagnostics.

---

## 3) Discover Entities

* For each **class** in <root>/models with `@D1`, mark it as an **entity**.
* Determine the **entity name** and compute the **table name** (allow `@D1({ table })` override).

---

## 4) Extract Fields

* For each **property**, map TypeScript → IR type. I left the 3-5 undefined for now to add more types later:

  | TS type   | IR `type`       |
  | --------- | -----------     |
  | `number`  | `int`  (0)      |
  | `string`  | `text` (1)      |
  | `boolean` | `bool` (2)      |
  | `Date`    | `ISO string` (3)|
  | `D1`      | `string`     (6)|
  | `Response`| `string`     (7)|

* Determine **nullability**. User must explicitly define nullability for now.

---

## 5) Extract Routes

* For each **method** with `GET(path)` or `POST(path)`:

  * Method: `GET` or `POST`.
  * Path: provided literal or default `/<entity>/<methodName>`.
  * Handler: `ClassName.method`.
  * Capture the **method body** and **parameter list** as a single **string** (to embed under `routes[].impl.code.source`).

---

## 6) Accumulate Resources

* **D1:** single item `{ "name": "default" }`.

---

## 7) Build Manifest Object

* Assemble `{ version, project, entities, routes, resources }` in memory.

---

## 8) Validation

* Validation will be handeled by code generator, but we should keep these in mind.

  * User must declare primary key using `@PrimaryKey` decorator.
  * Only the **allowed field types** (table above).
  * Routes **start with `/`** and **(method, path)** pairs are unique.
  * **Decorator arguments are literal-only** (no identifiers/expressions).


---

## 9) Emit

* Pretty-print the manifest to the <root>/.generated/cidl.json.
