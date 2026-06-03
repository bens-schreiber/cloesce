# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** Draft*| **Review** | Accepted | Rejected | Implemented
- **Created:** 2026-04-30
- **Last Updated:** 2026-06-03

---

# Summary

This proposal brings [Durable Objects](https://developers.cloudflare.com/durable-objects/) into Cloesce as first-class citizens. DOs open up a whole class of use cases that D1 just can't reach on its own: per-object isolated SQLite databases, strongly consistent state, and real-time WebSocket connections.

The gist: a new `durable` binding in the schema, the ability to back a Model with a DO (so the Model gets to use that instance's storage and SQLite database), automatic Worker-to-DO request forwarding for API methods, and generation for both Wrangler and SQL migrations.

---

# Motivation

Durable Objects are easily the most powerful product on the Cloudflare Workers platform, but they are also the hardest to use correctly. Without framework support, a developer must manually wire up routing, state initialization, migrations, and the Worker-to-DO request handoff, all before writing any application logic. This proposal aims to eliminate that boilerplate by integrating Durable Objects as first-class citizens in Cloesce's schema and code generation.

## The limits of D1

Cloesce currently supports D1 as its relational backend. D1 works well for many applications, but it has a fundamental architectural constraint: it is a single, globally shared database capped at 10 GB. This makes it a poor fit for applications where data is naturally partitioned by tenant, user, session, or resource (cases where you want the database to scale horizontally rather than vertically).

Durable Objects offer a different model entirely. Instead of one database, you get arbitrarily many. Each DO instance has its own isolated SQLite database, its own KV store, and its own compute context. A multi-tenant SaaS application, for example, can give each organization its own DO instance, with full data isolation and no shared contention.

## Strong consistency

D1 provides eventual consistency. A value must be updated atomically to ensure a valid state. Transactions do not exist.

Durable Objects on the other hand are strongly consistent: all requests to a given DO instance are processed sequentially, in order, on a single thread. This makes it straightforward to implement operations that would otherwise require careful locking or conflict resolution, like leaderboards, collaborative editing state, rate limiters, and similar workloads.

## Real-time communication

Because a DO instance is a persistent, long-lived object (not a stateless function invocation), it can hold open WebSocket connections and act as a coordination point between multiple clients. This proposal will not cover WebSocket support in this initial iteration, but will lay the groundwork for a future proposal that adds first-class WebSocket support to DO-backed Models.

---

# Goals and Non-Goals

## Goals

- Define a Durable Object Binding
  - Generate Wrangler configuration for DO bindings declared in the schema

- Back a Model with a Durable Object
  - Give a Model access to a Durable Object instance's storage and SQLite database
  - Execute a Model's REST API methods in the context of a Durable Object instance
  - Allow a Model to represent a SQLite table in a DO instance's database
  - Generate SQLite migrations

## Non-Goals

- Middleware for DO instance creation
- WebSocket RPC support
- Cross-shard relationships between DO-backed Models

---

# Design

Five axioms underlie the design of this proposal:

1. A Durable Object is not a Model.
2. A Model can only be backed by one Durable Object, but a Durable Object can back multiple Models.
3. Every shard of a Durable Object shares the exact same schema.
4. Given a serialized Model backed by a Durable Object, the client should be able to transparently invoke API methods on the same DO instance (RPC).
5. A Models relationships exist within each shard of a Durable Object, and cannot span across shards.

## Durable Object Bindings

To declare a Durable Object binding, a new `durable` block will be added to the schema:

```cloesce
durable CounterDo {
    shard {
        id: string
    }

    count() -> int {
        "count"
    }

    metadata(user_id: int) -> json {
        "metadata/{user_id}"
    }
}
```

A Durable Object binding is similar to KV, where it can define its own key and value fields. These are stored within the DO's instance storage.

Under the `shard` block, the DO can define a key structure for generating DO IDs. This allows a DO to be sharded by a specific key. Cloesce will generate a DO ID using fields from `shard`, which can specify any number of fields, or none at all (in which case the DO is global and not sharded).

The pattern for seeding a DO ID will follow `BINDING` followed by the `/` delimited concatenation of all shard fields. For example, the previous `CounterDo` would generate a DO ID like `CounterDo/{id}`.

## Backing a Model with a Durable Object

To "back" a Model with a Durable Object means that the Model exists within the context of a DO shard, and has access to that shards storage and SQLite database. This also means that in order to hydrate an instance of the Model, we must obtain an instance of the DO.

When a Model is backed by a DO, it has access to any fields and KV templates defined on the DO. For example:

For example:

```cloesce
durable CounterDo {
    shard {
        id: string
    }

    count() -> int {
        "count"
    }
}

kv Namespace {
    durableObjectRegistry(id: string) -> bool {
        "dos/{id}"
    }
}

model Counter for CounterDo {
    kv CounterDo::count() {
        count
    }

    kv Namespace::durableObjectRegistry(CounterDo::id) {
        isRegistered
    }
}
```

The `Counter` Model is backed by the `CounterDo` Durable Object, which means that each instance of `Counter` is associated with a specific instance of `CounterDo`. The `Counter` Model can access the DO's storage via the `count` template, and can also access the `CounterDo::id` field to look itself up in the `durableObjectRegistry` KV namespace.

In practice, the shard fields of a DO will be added to each model with the prefix `$` such that they cannot be confused with regular fields. For example, the `Counter` Model when generated to TypeScript would look like:

```ts
interface Self {
    $id: string; // from CounterDo::id
    // ...other fields
}
```

### KV

As shown in the above example, a Model backed by a Durable Object can access that DO's storage via the `kv` template.

Unlike D1, just because a Model is backed by a DO does not necessarily mean that it has to define some set of `primary` keys as a SQLite table.

A Durable Object backed Model can use `route` fields just like a Worker backed Model:

```cloesce
durable CounterDo {
    shard {
        id: string
    }

    count(id: string) -> int {
        "count/{id}"
    }
}

model Counter for CounterDo {
    route {
        counterId: string
        otherCounterId: string
    }

    kv CounterDo::count(counterId) {
        count
    }

    nav Counter::otherCounterId(otherCounterId) {
        otherCounter
    }
}
```

Here, the `Counter` Model has route fields `counterId` and `otherCounterId`, which must be provided to hydrate an instance of `Counter`. The `otherCounterId` field is used to navigate to another `Counter` instance.

It is assumed that `otherCounter` is inside of the same shard as `Counter`, and it will be hydrated as such.

### SQLite

A Model backed by a Durable Object can also represent a SQLite table in the DO instance's database:

```cloesce
durable BlogDo {
    shard {
        id: string
    }
}

model BlogPost for BlogDo {
    primary {
        id: int
    }

    column {
        title: string
        content: string
        created_at: datetime
    }

    nav (BlogComment::blog_post_id) {
        comments
    }
}

model BlogComment for BlogDo {
    primary {
        id: int
    }

    column {
        content: string
        created_at: datetime
    }

    foreign (BlogPost::id) {
        blog_post_id
    }
}
```

In this schema, each Blog has its own `BlogDo` instance, which means each Blog has its own isolated SQLite database. The `BlogPost` and `BlogComment` Models are tables that exist within that database, and the relationship between them is defined by the `nav` and `foreign` fields as usual.

Again, it is assumed that these relationships are all within the same shard.

## API Methods in a Durable Object

API Methods in Cloesce utilize the Cloesce Router. For example, a static method `foo` on some Model `Person` would be routed by the Cloesce Router like so:

```
Request (GET /Person/foo)
    -> Worker
        -> Router -> Match Route -> Validate Body -> Dispatch Method
            -> Person.foo(...)
```

If `foo` was to be an instance method, the routing would look like this:

```
Request (GET /Person/123/foo)
    -> Worker
        -> Router -> Match Route -> Validate Body -> Hydrate Model -> Dispatch Method
            -> person.foo(...)
```

An important feature of Durable Objects is that they execute in a separate context from the Worker. In order to route a request to some code capable of using the DO's storage or SQLite database, the request must be forwarded from the Worker to that DO instance.

Once an instance is located, the request must be forwarded to that instance. This means that every DO will need its own Cloesce Router to handle incoming requests forwarded from the Worker.

### Static Methods

A static method on a Model backed by a DO will still execute inside of the DO instance, which means it can access the DO's storage and SQLite database.

A key difference here is that if the DO is sharded, the static method will be forced to provide the shard fields as parameters in order for Cloesce to route the request to the correct DO instance.

For example, if a DO is sharded by an `id` field, then any static method on a Model backed by that DO must provide the `id` parameter.

The routing for a static method `foo` on a DO-backed Model `Person` would look like this:

```
Request (GET /Person/{id}/foo)
    -> Worker
        -> Router -> Match Route -> Validate Body -> Forward to DO Instance
            -> Router -> Match Route -> Validate Body -> Dispatch Method
                -> Person.foo(...)
```

### Instance Methods

An instance method on a Model backed by a DO will also execute inside of the DO instance, which means it can access the DO's storage and SQLite database, as well as the hydrated Model instance.

Just like static methods, if the DO is sharded, the instance method will be forced to provide the shard fields as parameters in order for Cloesce to route the request to the correct DO instance. These shard fields will be declared as `instance` fields on the Model, and such will not be explicitly passed in the generated API method signature.

The routing for an instance method `bar` on a DO-backed Model `Person` would look like this:

```
Request (GET /Person/{id}/{personId}/bar)
    -> Worker
        -> Router -> Match Route -> Validate Body -> Forward to DO Instance
            -> Router -> Match Route -> Validate Body -> Hydrate Model -> Dispatch Method
                -> person.bar(...)
```

### Router

Because a Durable Object is a separate execution context, it will require its own instance of the Cloesce Router. This means that when a request is forwarded from the Worker to the DO instance, the DO's Router will have to re-match the route and re-validate the body (if applicable) before dispatching to the correct method:

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

To stop some kind of infinite forwarding loop, a forwarded request will have a special header `x-cloesce-forwarded: true` that the Worker can check for. If a request with this header hits the Worker, it means that the request has already been forwarded once, and should not be forwarded again (throw a 500 error).

### Lazy Creation Problem

Cloudflare has no way to determine if a DO instance exists until a request is forwarded to it, at which point it will be created if it does not already exist.

For Cloesce's purposes, this is problematic. Any API method can create an entirely new DO instance, which is a security concern and can lead to unexpected costs if a route is hit with a new DO ID.

For a DO that is global, this is not a problem, because sharding is not necessary. However, for a sharded DO, some business logic must be put in place to prevent the creation of DO instances with invalid IDs.

This will be solved with middleware that runs before the request is forwarded to the DO instance, and accepts the same parameters as the DO's `shard` block.

A new middleware system for Cloesce will be proposed in a separate proposal.

### Registering Injected Instances

How can an injected dependency be shared between the Worker and the DO?

Any environment binding (D1, KV, DO, Vars) is by default shared between the Worker and DO, as Cloudflare populates the same `env` object for both. This means that if you have a DO binding, you can access it from the Worker and pass it to the DO as needed.

However, an instance defined in an `inject` block is not shared by Cloudflare, as it is purely a construct of Cloesce. The solution will be to trace the usage of each injected instance:

- If the instance is used in some DO-backed Model `D`, then that instance can be registered in the DO's router.
- If the instance is used in some non-DO-backed Model `W`, then that instance can be registered in the Worker's router.

If an injected instance is not used in any Model's API contract, then it cannot be registered in either router.

### Generated Backend Code

Any instance method of a Model backed by a Durable Object will be given the DO instance as the parameter `ctx`:

```ts
abstract class BlogDo extends DurableObject {
    id: string;
    constructor(ctx: DurableObjectState, env: Env, id: string) {
        super(ctx, env);
        this.id = id;
    }
}

export namespace BlogPost {
    // ...
    export interface Self {
        $id: string;
        id: number;
        title: string;
        content: string;
        created_at: Date;
    }

    // ...
    export interface Api {
        create(
            $ctx: BlogDo,
            title: string,
            content: string,
        ): ApiResult<Self>;

        update(
            self: Self,
            $ctx: BlogDo,
            title: string,
            content: string,
        ): ApiResult<Self>;
    }
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

Cloesce will not provide a runner for migrations (at least, not in this initial proposal), leaving it to the developer to decide how and when to run migrations on their DO instances.

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