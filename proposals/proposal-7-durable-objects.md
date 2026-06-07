# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** Draft | Review | Accepted | Rejected | Implemented
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
- WebSocket RPC generation
- Cross-shard relationships between DO-backed Models
- Cross-DO relationships between any Models (e.g. a Worker-backed Model using the KV of a DO instance)

---

# Design

Five axioms underlie the design of this proposal:

1. A Durable Object is not a Model.
2. A Model can only be backed by one Durable Object, but a Durable Object can back multiple Models.
3. Every shard of a Durable Object shares the exact same schema.
4. Given a serialized Model backed by a Durable Object, a client should be able to transparently invoke API methods on the same DO instance.
5. A Models relationships exist within each shard of a Durable Object, and cannot span across shards.

## Durable Object Bindings

> [!NOTE]
> Durable Object shard fields cannot be interpolated into KV templates

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

```ts
// GENERATED BACKEND TS CODE
export abstract class CounterDo implements DurableObject {
    protected id: string;

    constructor(public state: DurableObjectState, private env: Env) {
        const [, id] = state.id.toString().split("/");
        this.id = id;
    }

    static readonly Shard = {
        key: (id: string) => `CounterDo/${id}`,
        id: (namespace: DurableObjectNamespace, id: string) => namespace.idFromName(CounterDo.Shard.key(id)),
        get: (namespace: DurableObjectNamespace, id: string) => namespace.get(CounterDo.Shard.id(namespace, id)),
    }

    static readonly count = {
        key: () => "count",
        get: async (state: DurableObjectState) => await state.storage.get(CounterDo.count.key());
    };

    static readonly metadata = {
        key: (user_id: number) => `metadata/${user_id}`,
        get: async (state: DurableObjectState, user_id: number) => await state.storage.get(CounterDo.metadata.key(user_id));
    };
}
```

## Backing a Model with a Durable Object

To "back" a Model with a Durable Object means that the Model exists within a DO, and has access to the storage and SQLite database of that particular DO shard. This also means that in order to hydrate an instance of the Model, we must obtain an instance of the DO.

When a Model is backed by a DO, it has access to any fields and KV templates defined on the DO. For example:

```cloesce
durable CounterDo {
    shard {
        doId: string
    }

    count() -> int {
        "count"
    }
}

model Counter for CounterDo {
    kv CounterDo::count() {
        count
    }
}
```

The `Counter` Model is backed by the `CounterDo` Durable Object, which means that each instance of `Counter` is associated with a specific instance of `CounterDo`. The `Counter` Model can access the DO's storage via the `count` template.

In order to maintain the invariant that a Model backed by a DO must be associated with a specific DO instance, the `Self` interface of the Model will contain all shard fields of the DO.

```ts
interface Self {
    $doId: string; // from CounterDo::doId
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

It is assumed that `otherCounter` is inside the same shard as `Counter`, and it will be hydrated as such.

### SQLite

A Model backed by a Durable Object can also represent a SQLite table in the DO instance's database:

```cloesce
durable BlogDo {
    shard {
        doId: string
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

## API Methods

When a client invokes an API method on a Model, the request is received by a Worker instance (via HTTP), which then invokes the Cloesce Router.

For example, a static method `foo` on some Model `Person` would be routed like so:

```
Person.foo(...) -> Request GET /Person/foo
    -> Worker -> Router -> Match Route -> Validate Body -> Dispatch
            -> Person.foo(...)
```

An instance method `bar` on some Model `Person` would be routed like so:

```
person.bar(...) -> Request GET /Person/:personId/bar
    -> Worker -> Router -> Match Route -> Validate Body -> Hydrate Model -> Dispatch
            -> person.bar(...)
```

The key feature of a static method is that it does not require hydration of a Model instance, and therefore does not require access to a DO instance. Static methods will not be executed inside of a DO instance, using the Worker's compute context instead.

Instance methods on the other hand require hydration of a Model instance, which in turn requires access to a DO instance (since the Model is backed by a DO). Therefore, instance methods will be executed inside of a DO instance, using the DO's compute context instead of the Worker's.

For example, if `Person` is backed by a DO, the routing for an instance method `bar` would look like this:

```
person.bar(...) -> Request GET /Person/:doId/:personId/bar
    -> Worker -> Router -> Match Route -> Validate Body -> Hydrate stub -> Dispatch to stub
        -> Match Method -> Hydrate Model -> Dispatch
            -> person.bar(...)
```


```ts
// .cloesce/backend.ts
export abstract class PersonDo implements DurableObject {
    protected app: CloesceApp;
    protected id: string;

    constructor(public state: DurableObjectState, private env: Env) {
        const [, id] = state.id.toString().split("/");
        this.id = id;
    }

    static readonly Shard = {
         key: (id: string) => `PersonDo/${id}`,
         id: (namespace: DurableObjectNamespace, id: string) => namespace.idFromName(PersonDo.Shard.key(id)),
         get: (namespace: DurableObjectNamespace, id: string) => namespace.get(PersonDo.Shard.id(namespace, id)),
    }

    protected cloesce({
        Person:   // ...typed such that only PersonDo-backed Models can be registered here
    }) {
        // ... initialize the cloesce app
    }

    private async route(model: string, method: string, payload: any): Promise<Response> {
        return await this.app.handle(model, method, payload);
    }
}

export namespace Person {
    export interface Self {
        id: string; // from PersonDo::id
        // ...other fields
    }

    export interface Api {
        foo(env: { state: DurableObjectState }): ApiResult<void>
        bar(self: Self, env: { state: DurableObjectState }): ApiResult<void>;
    }
}

// main.ts
const Person = clo.Person.impl({
    // ...implementation of Person API methods
});

export class PersonDo extends clo.PersonDo {
    constructor(state, env) {
        super(state, env);
        this.app = super.cloesce({ Person });
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