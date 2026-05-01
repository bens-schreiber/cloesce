# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-04-30
- **Last Updated:** 2026-04-30

---

## Summary

[Durable Objects](https://developers.cloudflare.com/durable-objects/) are a Cloudflare Workers primitive that provide a way to model stateful objects in a distributed environment. Importantly, they enable a database-per-object model and are capable of maintaining WebSocket connections. This proposal details a first-class integration of Durable Objects into Cloesce.

---

## Motivation

Durable Objects are easily the most powerful and unique product on the Cloudflare Workers platform. Because Cloudflare has no true monolithic relational database (each D1 database is capped at 10 GB), Durable Objects are the best solution for stateful applications that rely on relational data. With a way to model a system of Durable Objects, Cloesce can be used in a wide variety of applications, including those that require real-time communication via WebSockets.

---

## Goals and Non-Goals

### Goals

- Define Durable Objects in the `env` schema declaration.
- Enable a Model to be backed by a Durable Object.
- Enable a Model to use a DO-based KV store for KV fields (instead of a KV bucket).
- Enable a Service to be backed by a Durable Object.
- Generate migrations for any SQL schema changes in a DO-backed Model.
- Generate Wrangler configuration for migrating and deploying DOs.

### Non-Goals

- Support for D1-backed models having relationships with DO-backed models (and vice versa).
- Support for DOs having relationships with different DOs.
- WebSocket RPC support (later!).
- Optimizing the `save` method for DO-backed models (the `_cloesce_tmp` table is used to store primary keys mid-transaction, but since a DO has strong consistency, this can likely be optimized).

---

## Detailed Design

### What is a Durable Object?

For the purposes of Cloesce, a Durable Object can be defined as:

- A class that extends the `DurableObject` class provided by Cloudflare.
- Maintains its own SQLite database and KV store (state).
- Strongly consistent; requests to the same DO instance are guaranteed to be processed sequentially, never concurrently.

Unlike a D1 database or KV/R2 bucket, DOs are not a single global resource (though you can design a system that way). Any number of DO instances can be created, each with its own state and ID.

Each DO class has a `fetch` method (much like a Worker's) that handles incoming requests. It is also common to use Cloudflare's built-in RPC system to call methods on a DO instance, but that will not be covered in this proposal.

### Defining a Durable Object Binding

A Durable Object is defined in the `env` schema declaration, much like a D1 database or KV bucket:

```cloesce
env {
    durable {
        MyDurableObject
        // other DOs...
    }
}
```

Note the casing convention. Unlike other bindings, a DO binding must compile to both an `env` binding and a class name, typically PascalCase:

```toml
[[durable_objects.bindings]]
name = "MyDurableObject"        # binding
class_name = "MyDurableObject"  # class name
```

For the purposes of this proposal, the binding name and class name will be kept the same.

### Durable Object-Backed Model

A Model can be "backed by" a Durable Object in the same way it can be backed by a D1 database. The DO is the source of truth: for a Model instance to exist, it must exist on a Durable Object instance.

```cloesce
[use MyDurableObject]
model User {
    primary {
        id: uint
    }

    kv (MyDurableObject, "user/{id}") {
        data: json
    }

    nav (Post::author) {
        posts
    }
}

[use MyDurableObject]
model Post {
    primary {
        id: uint
    }

    foreign (User::id) {
        author
    }
}
```

In the above code, the `User` model exists as a table in SQLite on a `MyDurableObject` instance. The `data` field is stored in the KV store of that same DO instance, with a key of `user/{id}`. Just like a D1-backed model, relationships can be defined between DO-backed models, provided they share the same backing DO.

### REST API Methods

Any Model backed by a DO can have REST methods defined within its `api` block:

```cloesce
[use MyDurableObject]
model Weather {
    ...
}

api Weather {
    post helloWorld() -> string
}
```

However, method execution must change. For a typical Model, the router follows this path within a Worker:

```
Request
    -> Worker -> Route -> Validation -> Hydrate -> Dispatch
```

A Worker route is registered like this:

```ts
export default {
    async fetch(request: Request, env: clo.Env): Promise<Response> {
        const app = (await clo.cloesce())
            .register(Weather);
        return await app.run(request, env);
    }
};
```

A Durable Object, however, does not exist within a Worker, it can only be *invoked* by one:

```
Request
    -> Worker -> Route -> Redirect
        -> DO -> Route -> Validate -> Hydrate -> Dispatch
```

Since a DO operates independently of a Worker, it needs its own `fetch` method and its own app registration:

```ts
export class MyDurableObject implements clo.MyDurableObject {
    async fetch(request, env): Promise<Response> {
        const app = (await clo.cloesce())
            .register(...);
        return await app.run(request, env);
    }
}

export default {
    async fetch(request: Request, env: clo.Env): Promise<Response> {
        const app = (await clo.cloesce())
            .register(...);
        return await app.run(request, env);
    }
};
```

### ORM

Every Durable Object instance is identified by a unique ID. It can be generated randomly (a 64-character hex string) or deterministically from a seed string (using `env.MyDurableObject.idFromName("some-string")`).

There are two ways to access a DO instance:

1. From outside the DO, using `env.MyDurableObject.get(id)`, which returns a stub for sending requests to that instance.
2. From inside the DO, using `this`, which provides direct access to the instance.

#### Backend API

A Model may exist on a DO, but it _is not_ the DO. The DO is just a database and compute environment that the Model uses. In this design, the DO instance and the Model instance are completely separate (no shared `this` context, no shared fields).

For example, a DO-backed Model with an API can be defined like this:

```cloesce
env {
    durable {
        MyDurableObject
    }
}

[use MyDurableObject]
model Foo {
    ...
}

api Foo {
    post id(e: env) -> string
}
```

Which will translate to:

```ts
export interface Env {
    MyDurableObject: DurableObjectNamespace;
}

export abstract class MyDurableObject implements DurableObject<Env> {
    abstract fetch(request: Request, env: Env): Promise<Response>;
}

export namespace Foo {
    // ...
    type InstanceEnv = Env & { $do: MyDurableObject };

    export interface Api {
        id(e: InstanceEnv, ...): ApiResult<string>;
    }

    export namespace Orm {
        async function get(env: InstanceEnv, ...);
        async function get(env: Env, $do: string | DurableObjectId, ...);
    }
}
```

The columns of `Foo` will never include `$do`, as that is a property of the backing DO instance, not the Model itself. Instead, to access that instance, you must pass `env` to the method — `env` includes a `$do` property identifying the DO instance on which the Model instance exists. Intentionally: `env` is not `self`.

#### Client API

Similar to the backend, any client method will need to specify how to locate the DO instance:

```ts
// given some Foo model with PK `id` that is backed by MyDurableObject...
class Foo {
    $do: string; // the DO id
    id: number;
    // other fields...

    static async $get($do: string, id: number) {...}
    static async $list($do: string, lastSeen_id: number, limit: number) {...}
    static async $save($do: string, data: DeepPartial<Foo>) {...}

    // has a saved $do
    async myMethod() {}
}
```

A DO-backed Service will behave similarly, with an additional DO ID parameter for each method:

```ts
// given some FooService that is backed by MyDurableObject...
class FooService {
    constructor (public $do: string) {}
    async myMethod() {...}
}
```

Previously, Services had entirely static methods. If backed by a DO, they must now be instantiated with a DO ID.

### Migrations

#### SQL

Since a DO-backed Model uses SQLite for relational storage, the same migration algorithm as a D1-backed Model applies. However, instead of running migrations via the Wrangler CLI, a Durable Object must run its own migrations on startup, per instance.

This means migrations for a DO will not be purely SQL: they will include HLL code to handle the migration logic.

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

Cloesce will provide a migration runner on each DO along the lines of:

```ts
type Migration = {
    name: string;
    timestamp: number;
    up: (db: D1Database) => Promise<void>;
}

export abstract class MyDurableObject implements DurableObject<Env> {
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

#### Durable Objects

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

Since Cloesce can track changes between migrations, whenever a DO is migrated (`cloesce migrate --binding MyDurableObject Name`), Cloesce can determine which of the above four operations is being performed and generate the appropriate Wrangler configuration. This command will also invoke any pending SQL migrations.
