# ORM Reference

Cloesce takes a different approach to the traditional ORM: focus on _hydrating data across cloud resources_ and _crud operations_, rather than hosting a complex query-builder.

Additionally, unlike other frameworks that combine an ORM with a REST API (such as Django, Rails, or Coalesce), Cloesce does _not_ use an [Active Record](https://en.wikipedia.org/wiki/Active_record_pattern) pattern, deliberately separating generated database types from database persistence.

## The Cloesce Environment

Every Cloudflare Workers application defines a set of [Environment Bindings](https://developers.cloudflare.com/workers/runtime-apis/#environment-bindings). These are available to the application at runtime.

Cloesce **upgrades** these bindings to provide a rich set of functionality for your application, and exposes them only to methods that explicitly inject them.

> [!TIP]
> The full set of upgraded bindings is exposed as the `env` property of the app, once a source has been bound with `worker` or `durable`.
> This allows you to utilize the bindings in your own middleware, or tests.
>
> ```ts
> import { createApp } from "@cloesce/backend.js";
>
> // ...
> const app = createApp().worker(env).register(...);
> await app.env.Db.Person.get(1);
> ```

> [!NOTE]
> Every property on the upgraded `env` is reached by exactly the name it is declared with in the schema (`Db`, `Person`, `MyKv`, ...).

## KV, R2, and Durable Object Methods

Key templates can be defined in [R2](./ch3-1-kv-and-r2.md#r2), [Durable Object KV](./ch3-3-durable-objects.md), and [Durable Object](./ch3-3-durable-objects.md) bindings.

Each upgraded binding exposes methods to read, write, and list data from these key templates.

```cloesce
durable MyDo {
    shard {
        tenant: string
    }

    settings -> json {
        id: string
        "custom/key/template/{id}"
    }
}

r2 MyBucket {
    image {}
}

kv MyKv {
    user -> json {
        id: int
        "user/{id}"
    }
}


// ...
api Person {
    get method {
        tenant: string

        inject {
            MyDo
            MyDo::tenant
            MyBucket
            MyKv
        }
    }
}
```

### KV

```ts
interface KvHelpers<T> {
  /** Renders the key template. */
  template(id: number): string;

  /** Reads the value at the templated key. */
  get(id: number): Promise<T | null>;

  /** Writes the value at the templated key. */
  put(id: number, value: T): Promise<void>;

  /** Lists keys under this template's prefix. */
  list(options?: {
    limit?: number;
    cursor?: string;
  }): Promise<{ keys: { key: number; value: T }[]; cursor?: string }>;
}
```

**Example Usage**

```ts
env.MyKv.user.template(1); // => "user/1"
await env.MyKv.user.put(1, { hello: "world" });
await env.MyKv.user.get(1); // => { hello: "world" } | null
await env.MyKv.user.list({ limit: 20 });
```

### R2

```ts
interface R2Helpers {
  /** Renders the key template. */
  template(): string;

  /** Reads the object's data. */
  get(): Promise<R2ObjectBody | null>;

  /** Writes the object's data. */
  put(
    value: ReadableStream | ArrayBuffer | ArrayBufferView | string | Blob,
  ): Promise<R2Object | null>;

  /** Lists object HEADs (not their data) under this template's prefix. */
  list(options?: {
    limit?: number;
    cursor?: string;
    delimiter?: string;
  }): Promise<{ objects: R2Object[]; cursor?: string }>;
}
```

**Example Usage**

```ts
env.MyBucket.image.template(); // => "image"
await env.MyBucket.image.put(new Uint8Array([1, 2, 3]));
await env.MyBucket.image.get(); // => R2ObjectBody | null
await env.MyBucket.image.list({ limit: 20 });
```

### Durable Object

```ts
interface DoHelpers<T> {
  /** Resolves the shard's DurableObjectId. */
  id(tenant: string): DurableObjectId;

  /** Resolves a stub for the shard, typed as `T`. */
  stub<T>(tenant: string): DurableObjectStub & T;
}
```

**Example Usage**

```ts
env.MyDo.id("tenant");
env.MyDo.stub<MyDo>("tenant");
```

### Durable Object KV

```ts
interface DoKvHelpers<T> {
  /** Renders the key template. */
  template(tenant: string): string;

  /**
   * Reads the value at the templated key.
   * Pass shard fields to call from outside the DO (async, routed over RPC).
   * Pass a `DurableObjectState` to call from inside the DO's own methods
   * (synchronous, reads local storage directly).
   */
  get(tenant: string): Promise<T | null>;
  get(ctx: DurableObjectState): T | null | undefined;

  /** Writes the value at the templated key. Same overload as `get`. */
  put(tenant: string, value: T): Promise<void>;
  put(ctx: DurableObjectState, value: T): void;

  /** Lists keys under this template's prefix. */
  list(ctx: DurableObjectState): { key: string; value: T }[];
}
```

**Example Usage**

```ts
env.MyDo.settings.template("tenant"); // => "custom/key/template/tenant"
await env.MyDo.settings.put("tenant", { hello: "world" }); // outside the DO
await env.MyDo.settings.get("tenant"); // outside the DO
```

## Model Methods

When a D1 or Durable Object database is injected into an API method, all Models defined against that database become available in their upgraded form, as `ModelStore` objects.

Every method on a `ModelStore` (`get`, `list`, `save`, `hydrate`, `hydrateAll`, `load`) returns an `HttpResult<T>`. This is the same result wrapper your API methods return:

```ts
const result = await env.Db.Person.get(1);
if (!result.ok) return result; // propagate the 404/400
const person = result.data!;
```

### Default and Named Data Sources

Every `ModelStore` exposes the model's **Default Data Source** directly as `get`, `list`, and `save`:

```ts
await env.Db.Person.get(1); // Promise<HttpResult<Person>>
await env.Db.Person.list(0, 20); // Promise<HttpResult<Person[]>>
await env.Db.Person.save({ id: 1, name: "Ada", age: 30 }); // Promise<HttpResult<Person>>
```

Every other Data Source is available as its own property, keyed by its declared name. It exposes the same `get`/`list`/`save` shape, scoped to that source's include tree and parameters:

```ts
await env.Db.Person.OverAge.list(18, 0, 20); // scoped to the `OverAge` source
```

### `hydrate` and `hydrateAll`

Both the Default Data Source and every named Data Source expose `hydrate` and `hydrateAll`. These turn partial, already-fetched rows into fully hydrated Models according to that source's include tree.

- `hydrate(row)`: takes one partially-loaded row, mutated in place and consumed. Fills in whatever relations aren't already present on it.
- `hydrateAll(rows)`: same, but for an array of rows. The array becomes the complete root set, so no root fetch is issued. Only the missing relations are fetched.

> [!TIP]
> Hydration is guided by what's already on the row. A field or relation that's already present (even as `[]`) is treated as authoritative and skipped.
>
> This lets you interleave hand-written queries with the ORM. Write your own SQL for a custom filter, then hand the raw rows to `hydrateAll` to fill in everything else: R2 fields, KV fields, related Models across other bindings.

```cloesce
r2 Avatars {
    avatar {
        pId: int
    }
}

model Person for Db {
    primary {
        id: int
    }

    column {
        name: string
        age: int
    }

    r2 Avatars::avatar(id) {
        avatar
    }
}

source OverAge for Person {
    include {
        avatar
    }

    list {
        age: int
        lastId: int
        limit: int

        inject { Db }
    }
}
```

To implement the `OverAge::list` method, you could do the following:

```ts
import { Api } from "@cloesce/backend.js";

export const overAge = {
  async list(env, age, lastId, limit) {
    // Get the list of people over 18 from the database
    const res = await env.Db.prepare(
      `SELECT * FROM Person WHERE age > ?1 AND id > ?2 ORDER BY id ASC LIMIT ?3`,
    )
      .bind(age, lastId, limit)
      .all();

    // Ta-da! Cloesce turned all of the database row results into fully hydrated Person
    // instances, with their R2 fields populated.
    return env.Db.Person.OverAge.hydrateAll(res.results);
  },
} satisfies Api.Person.OverAge.Of;
```

### `load`

Unique to the `ModelStore` itself (not on named Data Sources) is `load`. It takes a `Model` value you already hold and an ad-hoc `IncludeTree`, and returns a hydrated copy without consuming or mutating the original.

Where `hydrate`/`hydrateAll` run a source's precompiled include tree, `load` plans its tree at runtime. You can shape it per call:

```ts
async feed(self, env) {
  const full = await env.SubRedditDb.SubReddit.load(self, {
    posts: {
      post: {
        meta: {},
        comments: {},
      },
    },
  });
  return full.data?.posts.map((p) => p.post) ?? [];
}
```

`load` is the escape hatch for cases a compile-time Data Source doesn't cover, like hydrating different relations depending on a runtime flag.

## Cloesce Query Planner

How the heck does Cloesce know how to hydrate a Model, with data that could be stored _anywhere_?

Cloesce splits query work into two parts:

- The **Query Planner** looks at a Model's schema and an include tree, and decides what to fetch, from where, and in what order.
- The **Query Executor** walks that plan at runtime, and issues the reads and writes.

Plans are of two different IR forms: `select` and `save`.

### Select Plans

Select plans are either generated at _compile time_ (for every Data Source), or can be dynamically made at _runtime_ (for the `load` method). The runtime path is a call to a WASM module that implements the planner.

Plans consist of **stages**: a sequence of operations that must be performed in order, blocking on each stage until the next stage can be executed.

Every stage consists of one or more **steps**: a single operation that can be executed in parallel with other steps in the same stage.

The planner's goal is to minimize the number of stages, and maximize the number of steps in each stage, such that as much work as possible can be done in parallel.

> [!NOTE]
> Nested `many` relationships are batch-loaded such that the `N+1` query problem is mitigated.

### Save Plans

Save plans work in the other direction. Given a partial payload to `save`, the planner figures out:

- What to `INSERT`/`UPDATE`, and where.
- In what order writes must happen, so foreign keys and shard fields resolve correctly.
- What needs to be **read back** afterward (e.g. an autoincrementing primary key) before a dependent write can use it.

Save plans can't be fully precompiled. The shape of what's being saved depends on which fields are present in a given call's payload, so they're planned once per call, at runtime.

Writes to the same backend that don't depend on each other's readback values are grouped into a single **batch**. Writes that need a value produced by an earlier stage, like a just-inserted parent ID referenced as `saved.board.pid`, wait for the stage that produces it.

### Explain Command

The `cloesce explain` CLI command prints the exact plan the planner produced for a given Model, Data Source, and operation.

The same text is embedded as the `@remarks` block on the generated method's JSDoc. It also shows up in your editor's hover tooltip.

```sh
cloesce explain <model> <data_source> <get|list|save> [--dir .] [--payload <file.json>]
```

- `get`/`list` print the precompiled plan directly, no payload needed.
- `save` requires `--payload <file.json>`, a JSON body shaped like what you'd pass to that source's `save` method, since the plan depends on the payload's shape.

Each step in the printed tree is one operation. Here's what the grammar means:

| Term             | Meaning                                                                               |
| ---------------- | ------------------------------------------------------------------------------------- |
| `SEARCH`         | Reads rows matching a predicate (a `WHERE` clause).                                   |
| `SCAN`           | Reads rows with no predicate, an unfiltered read.                                     |
| `READ` / `WRITE` | Reads or writes a single KV/DO-KV/R2 key.                                             |
| `BATCH ON`       | A group of SQL statements sent to one database in a single round trip.                |
| `INSERT`         | An insert within a `BATCH`, with its column values shown.                             |
| `READBACK`       | Re-reads a row just written, to pick up generated values like an autoincrementing id. |
| `SYNTHESIZE`     | Assembles a result from values already on hand, no fetch needed.                      |
| `INTO`           | Where the result of this step lands in the hydrated tree.                             |
| `KEY`            | The resolved key template for a KV/DO-KV/R2 operation.                                |
| `JOIN`           | How child rows are matched back to their parent (`parent.field` = `row.field`).       |
| `SHARD`          | Which fields select the Durable Object shard for this operation.                      |
| `ATTACH`         | Extra fields carried onto the result alongside the fetched row.                       |
| `VALUE`          | The literal value being written.                                                      |
| `ONE` / `MANY`   | Whether the step expects a single row or a list.                                      |

`cloesce explain Org Default get` prints:

```
SELECT PLAN (GET) `Org` · 3 stages · 5 steps
INCLUDE
└─ `board`
   ├─ `banner`
   ├─ `entries`
   └─ `top`

STAGE 0
└─ SEARCH `Org` ON d1 `db` ONE

STAGE 1
├─ SCAN `Board` ON durable `BoardDo` INTO `board` ONE
│     JOIN `parent.tenantId` = `row.tenantId`
│     SHARD `tenantId` = tenantId
│     ATTACH `tenantId` = tenantId
└─ READ durable `BoardDo` KEY "top" INTO `board.top` SHARD `tenantId` = tenantId

STAGE 2
├─ READ r2 `Bucket` KEY "banners/{board.pid}" INTO `board.banner`
└─ SEARCH `Entry` ON durable `BoardDo` INTO `board.entries` MANY
      JOIN `parent.tenantId` = `row.tenantId` AND `parent.pid` = `row.boardId`
      SHARD `tenantId` = tenantId
      ATTACH `tenantId` = tenantId
```

`cloesce explain Org Default save --payload payload.json` prints the save plan. The payload here creates an `Org` along with its `Board`, one `Entry`, and a KV-backed `top`:

```
SAVE PLAN `Org` · 1 stage · 3 steps
INCLUDE
└─ `board`
   ├─ `banner`
   ├─ `entries`
   └─ `top`

STAGE 0
├─ BATCH ON d1 `db`
│  ├─ INSERT `Org` (`tenantId` = 7)
│  └─ READBACK `Org` INTO `result`
├─ BATCH ON durable `BoardDo` SHARD `tenantId` = 7
│  ├─ INSERT `Board` DEFAULT VALUES
│  ├─ INSERT `Entry` (`score` = 42, `boardId` = `saved.board.pid`)
│  ├─ READBACK `Board` INTO `board`
│  └─ READBACK `Entry` INTO `board.entries[0]`
└─ WRITE durable `BoardDo` KEY "top" INTO `board.top` SHARD `tenantId` = 7
   └─ VALUE {"cached":true}
```

> [!NOTE]
> `Entry`'s insert shares a stage with `Board`'s insert-and-readback because it's batched onto the same Durable Object round trip.
>
> `banner` appears in the include tree but produces no step: R2 fields are read-only, so a save never writes to a bucket.

> [!WARNING]
> **`save` does not write R2 objects.** An R2 field hydrates as an `R2Object` HEAD metadata such as `key`, `size`, and `etag`, not the object's bytes, so there is no value a save payload could carry that would meaningfully round-trip into the bucket. Including an R2 field in a save payload is silently ignored.
>
> Upload through the bucket binding directly:
>
> ```ts
> await env.Avatars.avatar.put(user.id, bytes);
> ```
>
> Reads are unaffected: an included R2 field still hydrates on `get`, `list`, and `hydrateAll`.
