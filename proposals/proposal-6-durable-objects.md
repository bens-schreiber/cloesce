# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-04-30
- **Last Updated:** 2026-04-30

---

# Summary

This proposal brings [Durable Objects](https://developers.cloudflare.com/durable-objects/) into Cloesce as first-class citizens. DOs open up a whole class of use cases that D1 just can't reach on its own: per-object isolated SQLite databases, strongly consistent state, and real-time WebSocket connections.

The gist: a new `durable` binding in the schema, the ability to back a Model with a DO (so the Model gets to play with that instance's storage and SQLite database), automatic Worker-to-DO request forwarding for API methods, and generation for both Wrangler and SQL migrations.

Additionally, a handful of older schema constructs (`keyfield`, the `env` block, the `paginated` infix keyword, the `use` tag) are getting retired in favor of bindings that own their own key and value structure, plus a new `for` keyword for pinning a Model to its backing store.

---

# Motivation

Durable Objects are easily the most powerful product on the Cloudflare Workers platform, but they are also the hardest to use correctly. Without framework support, a developer must manually wire up routing, state initialization, migrations, and the Worker-to-DO request handoff, all before writing any application logic. This proposal aims to eliminate that boilerplate by integrating Durable Objects as first-class citizens in Cloesce's schema and code generation.

## The limits of D1

Cloesce currently supports D1 as its relational backend. D1 works well for many applications, but it has a fundamental architectural constraint: it is a single, globally shared database capped at 10 GB. This makes it a poor fit for applications where data is naturally partitioned by tenant, user, session, or resource (cases where you want the database to scale horizontally rather than vertically).

Durable Objects offer a different model entirely. Instead of one database, you get arbitrarily many. Each DO instance has its own isolated SQLite database, its own KV store, and its own compute context. A multi-tenant SaaS application, for example, can give each organization its own DO instance, with full data isolation and no shared contention.

## Strong consistency

D1 (and most distributed databases) provides eventual consistency. A value must be updated atomically to ensure a valid state. Transactions do not exist.

Durable Objects, on the other hand, are strongly consistent: all requests to a given DO instance are processed sequentially, in order, on a single thread. This makes it straightforward to implement operations that would otherwise require careful locking or conflict resolution, like leaderboards, counters, collaborative editing state, rate limiters, and similar workloads.

## Real-time communication

Because a DO instance is a persistent, long-lived object (not a stateless function invocation), it can hold open WebSocket connections and act as a coordination point between multiple clients.

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
- WebSocket RPC support (later!)

---

# Design

Several axioms underlie the design of this proposal:

1. A Durable Object is not a Model.
2. A Model can only be backed by one Durable Object, but a Durable Object can back multiple Models.
3. If a Model is backed by a Durable Object, every single instance of that Durable Object is capable of hydrating that Model.
4. Given a serialized Model backed by a Durable Object, the client should be able to transparently invoke API methods on the same DO instance.

## Breaking Refactors

Currently, a Model must declare how it sources data from R2 or KV:

```cloesce
env {
    r2 {
        avatars
    }

    kv {
        metadata
    }
}

model User {
    keyfield {
        id: int
    }

    r2 (avatars, "key/{id}") {
        avatar
    }

    kv (metadata, "metadata/{id}") {
        meta: json
    }

    kv (metadata, "metadata/") paginated {
        metas: json
    }
}
```

This has several problems:

1. `keyfield` does not exist anywhere. `User` instances cannot be composed in a Model because there is no way to know how to hydrate the nested Model's keyfield.
2. If another Model were to need an avatar or some metadata, it would have to declare its own R2 and KV fields (because we cannot compose non-relational Models), which leads to boilerplate and potential inconsistencies.
3. CRUD methods like `list` cannot be generated for `User` because there is no way to know how to generate the keys for listing all users in R2 or KV.

The proposed solution is to remove the `keyfield` completely, and instead, allow environment bindings to house their own key and value fields. For example:

```cloesce
r2 UserAvatars {
    avatar(id: int) {
        "key/{id}"
    }
}

kv UserMetadata {
    meta(id: int) -> json {
        "metadata/{id}"
    }

    // note: can remove the `paginated` infix keyword on models now that the binding
    // itself can define the key structure for listing
    metas() -> paginated<json> {
        "metadata/"
    }
}

// D1 bindings will stay the same
d1 {
    UserDb
}
```

The `env` block can be removed as well, with all bindings declared at the top level of the schema.

How can we have a `User` Model if it is not backed by D1? In this proposal, we will allow a Model to be backed by a Durable Object, which will give it access to the DO's instance storage and SQLite database. Another approach to this could be the Service pattern:

```cloesce
poo User {
    id: int
    avatar: r2object
    meta: json
}

model UserService {}

api UserService {
    get user(id: int) -> User
    get avatar(id: int) -> stream
}
```

Internally, the `UserService` will hydrate a `User` manually by utilizing the new generated methods of the `UserAvatars` and `UserMetadata` bindings.

### Referencing a KV or R2 field

To reference a field from a KV or R2 binding, a new syntax will replace the current `kv` and `r2` blocks on a Model:

```cloesce
model User {
    primary {
        id: int
    }

    kv UserMetadata::meta(id) {
        meta
    }

    r2 UserAvatars::avatar(id) {
        avatar
    }
}
```

Each declaration must specify the binding being used, as well as the parameters needed to generate the key for that field. If a parameter in the `kv` or `r2` declaration takes some set of validator tags, those same tags _must_ be declared on the field of the Model that is passed.

### Removing the `use` tag

The `use` tag on a Model will be removed in favor of the following syntax:

```cloesce
model User for UserDb {
    // ...
}
```

Unlike the ambiguous `use` tag, `for` makes it clear that this Model is for a specific binding, and that it is backed by that binding.

## Durable Object Bindings

To declare a Durable Object binding, a new `durable` block will be added to the schema:

```cloesce
durable CounterDo {
    primary {
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

Under the `primary` block, the DO can define a key structure for generating DO IDs. This allows a DO to be sharded by a specific key. Cloesce will generate a DO ID using fields from `primary`, which can be composite or omitted entirely.

The pattern for seeding a DO ID will follow `BINDING` followed by the `/` delimited concatenation of all primary fields. For example, the previous `CounterDo` would generate a DO ID like `CounterDo/{id}`.

## Backing a Model with a Durable Object

To "back" a Model with a Durable Object means that the Model exists within the context of a DO instance, and has access to the DO instance's storage and SQLite database. This also means that in order to hydrate an instance of the Model, we must obtain an instance of the DO.

By default, every value defined under the DO binding's `primary` block will exist as a field on that Model, prefixed with a `$`. Additionally, a Model that is backed by a DO can access all of the DO's storage fields by referencing them in the schema with the `kv` syntax.

For example:

```cloesce
durable CounterDo {
    primary {
        id: string
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

The above code defines a `CounterDo` which is sharded by an `id` string. The Model `Counter` is backed by `CounterDo`, which means it has access to the DO's instance storage, as well as the `id` field of the DO (serialized as `$id` on the Model).

### SQLite

A Model backed by a Durable Object can also represent a SQLite table in the DO instance's database. This allows the Model to define relationships to other Models backed by the same DO.

```cloesce
durable BlogDo {
    primary {
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

Unlike the `keyfield` problem, where a Model could not be hydrated because there was no way to determine what the keys were, we know that a Model backed by a DO would only be hydrated within the context of a DO instance.

## API Methods in a Durable Object

API Methods in Cloesce utilize the Cloesce Router, a lightweight router that simply follows what the schema defines, given some request. For example, a static method on some Model `Person` would be routed by the Cloesce Router like so:

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

An important feature of Durable Objects is that they execute in a separate context from the Worker. That means that in order to route a request to some code capable of using the DO's storage or SQLite database, the request must be forwarded from the Worker to that DO instance.

Static methods on a Model backed by a DO will be routed to the DO instance, and have access to the DO's storage and SQLite database. Instance methods will also be routed to the DO instance, but they will additionally have access to the hydrated Model instance.

```cloesce
durable BlogDo {
    primary {
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
        created_at: date
    }
}

api BlogPost {
    post create(title: string, content: string) -> BlogPost
    post update(self, title: string, content: string) -> BlogPost
}
```

The above `BlogPost` Model describes two methods:

- `create`: A static method that executes inside of the DO instance, which means it can access the DO's storage and SQLite database.
- `update`: An instance method that executes inside of the DO instance, which means it can access the DO's storage and SQLite database, as well as the hydrated `BlogPost` instance.

The routing for these methods would look like this:

```
Request (POST /BlogPost/{doId}/create)
    -> Worker
        -> Router -> Match Route -> Validate Body -> Forward to DO Instance
            -> Router -> Match Route -> Validate Body -> Hydrate Model -> Dispatch Method
                -> BlogPost.create(...)
```

```
Request (POST /BlogPost/{doId}/{blogPostId}/update)
    -> Worker
        -> Router -> Match Route -> Validate Body -> Forward to DO Instance
            -> Router -> Match Route -> Validate Body -> Hydrate Model -> Dispatch Method
                -> blogPost.update(...)
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

This will be solved with middleware that runs before the request is forwarded to the DO instance, and accepts the same parameters as the DO's `primary` block.

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
    $id: string;
    constructor(ctx: DurableObjectState, env: Env, $id: string) {
        super(ctx, env);
        this.$id = $id;
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

---

# Implementation

The implementation for this proposal will be significant, broken down into the following phases:

1. Breaking refactors: remove `keyfield`, `paginated` infix keyword, `env` block, `use` tag, and add new syntax for referencing KV/R2 fields and backing a Model with a DO.
2. Durable Object bindings, with basic Wrangler configuration generation.
3. Durable Object-backed Models with KV capabilities and API methods.
4. SQLite support including migrations.