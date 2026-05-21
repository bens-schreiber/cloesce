# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-04-30
- **Last Updated:** 2026-04-30

---

# Summary

This proposal adds first-class support for [Durable Objects](https://developers.cloudflare.com/durable-objects/) in Cloesce. Durable Objects unlock use cases that D1 alone cannot address: per-object isolated SQLite databases, strongly consistent state, and real-time WebSocket connections. The proposal covers how to declare DO bindings in the `env` schema, back a Model against a DO, route REST API methods through a DO, and generate both SQL and Wrangler migrations as DO classes evolve.

---

# Motivation

Durable Objects are easily the most powerful product on the Cloudflare Workers platform, but they are also the hardest to use correctly. Without framework support, a developer must manually wire up routing, state initialization, migrations, and the Worker-to-DO request handoff, all before writing any application logic. This proposal aims to eliminate that boilerplate by integrating Durable Objects as first-class citizens in Cloesce's schema and code generation.

## The limits of D1

Cloesce currently supports D1 as its relational backend. D1 works well for many applications, but it has a fundamental architectural constraint: it is a single, globally shared database capped at 10 GB. This makes it a poor fit for applications where data is naturally partitioned by tenant, user, session, or resource (really, cases where you want the database to scale horizontally rather than vertically).

Durable Objects offer a different model entirely. Instead of one large database, you get arbitrarily many small ones. Each DO instance has its own isolated SQLite database, its own KV store, and its own compute context. A multi-tenant SaaS application, for example, can give each organization its own DO instance, with full data isolation and no shared contention.

## Strong consistency

D1 (and most distributed databases) provides eventual consistency. A value must be updated atomically to ensure a valid state. Transactions do not exist.

Durable Objects, on the other hand, are strongly consistent: all requests to a given DO instance are processed sequentially, in order, on a single thread. This makes it straightforward to implement operations that would otherwise require careful locking or conflict resolution, like leaderboards, counters, collaborative editing state, rate limiters, and similar workloads.

## Real-time communication

Because a DO instance is a persistent, long-lived object (not a stateless function invocation), it can hold open WebSocket connections and act as a coordination point between multiple clients.

---

# Goals and Non-Goals

## Goals

- Define Durable Objects in the `env` schema declaration.
- Enable a Model to be backed by a Durable Object.
- Enable a Model to use a DO-based KV store for KV fields (instead of a KV bucket).
- Generate migrations for any SQL schema changes in a DO-backed Model.
- Generate Wrangler configuration for migrating and deploying DOs.

## Non-Goals

- Support for D1-backed Models having foreign keys to DO-backed Models (and vice versa).
- WebSocket RPC support (later!).
- Optimizing the `save` method for DO-backed Models (the `$cloesce_tmp` table is used to store primary keys mid-transaction, but since a DO has strong consistency, this can likely be optimized).

---

# Design

## Defining a Durable Object Binding

Unlike D1, KV, and R2 bindings, which represent global singleton resources, a DO binding can have any number of instances. Thus, a DO binding must include some way to define how its instances are identified.

There are several common patterns for DO instance identification:

1. **Raw DO ID**: Every DO has an underlying unique 64-character hex ID, which can be generated randomly.
2. **Static ID**: Generated from some constant seed string.
3. **Dynamic ID**: Generated from some combination of parameters determined at runtime.

Cloesce needs to support all three of these patterns. Defining a DO binding in the schema will follow this pattern:

```cloesce
durable MyRawDo {
    keyfield {
        raw: doid
    }

    primary (raw)
}

durable MyStaticDo {
    primary ("global_counter")
}

durable MyDynamicDo {
    keyfield {
        counter_id: string
    }

    primary (counter_id)
    // string interpolation is available too: primary ("counter/{counter_id}")
}
```

In the above example, three DO bindings are defined:

| Durable Object | ID Type    | ID Source             | Parameters          | Instance Behavior                                                         |
| -------------- | ---------- | --------------------- | ------------------- | ------------------------------------------------------------------------- |
| `MyStaticDo`   | Static ID  | `global_counter`      | None                | Only one instance exists                                                  |
| `MyDynamicDo`  | Dynamic ID | `counter_id` keyfield | `counter_id`        | Multiple instances can exist, each identified by a different `counter_id` |
| `MyRawDo`      | Raw ID     | `raw` key             | `raw` (`doid` type) | Uses a raw Durable Object ID directly                                     |

A new type, `doid`, will be introduced to represent a raw Durable Object ID. It is SQLite-compatible and runtime-validated to ensure it conforms to the expected format.

### Environment Binding Syntax Changes

Moving forward, we will remove the `env` block. Instead, all definitions will be at the top level of the schema. For example:

```cloesce
d1 {
    my_db
    my_other_db
}

kv {
    my_bucket
}

durable MyDurableObject {
    keyfield {
        name: string
    }

    primary ("counter/{name}")
}
```

## Durable Object Field

Any Model may have a field that hydrates from a Durable Object, much like KV and R2 fields:

```cloesce
durable Counter {
    keyfield {
        id: int
    }

    primary ("counter/{id}")
}

model MyModel {
    keyfield {
        counter_id: int
    }

    durable (Counter, counter_id) {
        counter
    }
}
```

When a durable field is declared, semantic analysis will ensure that each of its `primary` fields is satisfied in the definition.

During hydration, the `counter` field of `MyModel` will be hydrated with an instance of the Durable Object `Counter`. If the DO does not exist, the field will be hydrated with `null` instead.

Durable Object fields can be included in or excluded from a Data Source's Include Tree, but will always be hydrated in the Default Data Source.

## Durable Object Backed Models

A Model can be backed by a DO, much like you can back a Model against a D1 database.

- If the backing DO is not found, then any attempt to hydrate an instance of that Model will return a 404.
- A DO-backed Model will inherit all key fields of the backing DO.
- If a SQL field is declared, then the DO will be initialized with a SQLite database, and that field will be stored in a table in that database.
- KV fields on a DO-backed Model can use the DO's KV store.

For example:

```cloesce
durable CounterDo {
    keyfield {
        id: int
    }

    primary (id)
}

[use CounterDo]
model Counter {
    kv (self, "count") {
        count: int
    }
}
```

The `self` keyword is used to indicate that the `count` field should be stored in the DO's KV store, rather than some global KV bucket.

Because a Model must be serializable to the client and invokable remotely, all fields of the backing DO must exist as fields on the Model, so that Cloesce can locate the backing DO. The compiler will handle this implicitly during a post-semantic expansion step.

### SQLite Usage

A DO-backed Model that defines any SQL fields will exist as a SQLite table in the DO instance's database. For example:

```cloesce
durable Blog {
    keyfield {
        blogId: string
    }

    primary ("blog/{blogId}")
}

[use Blog]
model Post {
    primary {
        id: int
    }

    foreign (Comments::postId) {
        comments
    }
}

[use Blog]
model Comment {
    primary {
        id: int
    }

    foreign (Post::id) {
        post
    }
}
```

Here, both `Post` and `Comment` are backed by the `Blog` DO, and both will be stored as tables in the DO's SQLite database. The foreign key relationship between them is maintained as normal, but now all queries and mutations on those tables will execute within the context of the DO instance.

## REST API Methods

A DO-backed Model can define REST API methods just like any other Model. However, where those methods execute will be different: instead of executing within the context of a Worker, they will execute within the context of the DO instance that backs the Model.

The request flow for a Worker-based Model looks like this:

```
Request
    -> Worker -> Route -> Validation -> Hydrate -> Dispatch -> Response
```

For a DO-backed Model, the flow will look like this:

```
Request
    -> Worker -> Route
        -> DO -> Route -> Validate -> Hydrate -> Dispatch
    -> Response
```

To make this work, both the Worker and the DO will need their own Cloesce Router. To make their API contracts apparent to the developer, a new syntax will be introduced for registering implementations:

```ts
export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = await clo.cloesce({
      MyWorkerModel: MyWorkerModelImpl,
      // ...typed such that only Worker-based Models can be registered here
    });
    return await app.run(request, env);
  },
};

export class MyDurableObject implements clo.MyDurableObject {
  async fetch(request, env): Promise<Response> {
    const app = await clo.cloesce({
      MyDoBackedModel: MyDoBackedModelImpl,
      // ...typed such that only DO-backed Models can be registered here
    });
    return await app.run(request, env);
  }
}
```

### Registering Injected Instances

How can an injected dependency be shared between the Worker and the DO?

Any environment binding (D1, KV, DO, Vars) is by default shared between the Worker and DO, as Cloudflare populates the same `env` object for both. This means that if you have a DO binding, you can access it from the Worker and pass it to the DO as needed.

However, an instance defined in an `inject` block is not shared by Cloudflare, as it is purely a construct of Cloesce. The solution will be to trace the usage of each injected instance:

- If the instance is used in some DO-backed Model `D`, then that instance can be registered in the DO's router.
- If the instance is used in some non-DO-backed Model `W`, then that instance can be registered in the Worker's router.

If an injected instance is not used in any Model's API contract, then it cannot be registered in either router.

### Injecting a Durable Object Environment Binding

Since a DO's binding is part of the environment, it can be injected like any other environment binding. However, this does not inject an instance of the DO itself, but rather the binding that allows you to interact with DO instances (e.g. to forward requests to them, or to fetch their state). For example:

```cloesce
api MyModel {
    [inject MyDurableObject]
    get do_binding() -> string
}
```

## Migrations

A Durable Object can be migrated in two ways: changes to the DO class's underlying Wrangler configuration, and changes to the DO's SQLite schema.

### SQL

SQL migrations cannot be applied to a DO via a Wrangler CLI command (unlike a D1 database). This means that migrations for a DO cannot be purely SQL: they must transpile to HLL code that runs on the DO instance itself, which applies the necessary SQL changes to the DO's SQLite database.

An example migration:

```ts
// <binding>/<timestamp>_<migration name>.ts
async function up(db) {
    await db.prepare(
        `ALTER TABLE users ADD COLUMN age INTEGER ...`
    );
}
export default {
    name: "migrationName",
    timestamp: 1234567890,
    id: "migrationName_timestamp",
    up
}
```

Cloesce will provide a migration runner on each DO, along the lines of:

```ts
type Migration = {
    name: string;
    timestamp: number;
    up: (db: D1Database) => Promise<void>;
}

export abstract class MyDurableObject implements DurableObject<Env> {
    // ... keyfields
    // ... methods to get a DO instance by id

    protected async migrate(ctx: DurableObjectState, migrations: Migration[]) {
        ctx.blockConcurrencyWhile(async () => {
            // Check this DO's KV storage for each migration to see if it has been run before.
            // If not, add it to the list of pending migrations.
            const toMigrate = await Promise.all(
                migrations.map(async (m) => await ctx.storage.get(m.id) ? null : m)
            );

            // Run each pending migration in order of timestamp.
            const sorted = toMigrate.filter(m => m !== null).sort((a, b) => a.timestamp - b.timestamp);

            for (const m of sorted) {
                await m.up(ctx.storage);
                await ctx.storage.put(m.id, true);
            }
        });
    }

    abstract fetch(request: Request, env: Env): Promise<Response>;
}
```

This allows a developer to run migrations on a DO instance like so:

```ts
class MyDurableObject implements clo.MyDurableObject {
    constructor(state: DurableObjectState, env: Env) {
        this.migrate(state, [
            // list of migrations to run, imported from the migrations directory
        ]);
    }

    async fetch(request, env) {
        ...
    }
}
```

### Wrangler

A Durable Object class can evolve in four ways:

1. Creating
2. Renaming
3. Modifying SQL support
4. Deleting

For example:

```toml
[[migrations]]
tag = "v1"
new_sqlite_classes = ["MyDO"]

[[migrations]]
tag = "v2"
new_sqlite_classes = ["UserDO", "OrgDO"]

[[migrations]]
tag = "v3"
renamed_classes = [
  { from = "MyDO", to = "SessionDO" }
]

[[migrations]]
tag = "v4"
deleted_classes = ["OrgDO"]
```

---

# Implementation

The implementation for this proposal will be significant, broken down into the following phases:

1. Define Durable Object bindings, with basic Wrangler configuration generation.
2. Add Durable Object fields to the schema, and hydrate them in the runtime ORM.
3. APIs execute in DO instances for DO-backed Models.
4. KV fields can use the DO's KV store.
5. SQL migrations for DO-backed Models.
6. Wrangler migrations.
