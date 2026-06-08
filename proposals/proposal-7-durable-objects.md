# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** Draft | **Review** | Accepted | Rejected | Implemented
- **Created:** 2026-04-30
- **Last Updated:** 2026-06-03

---

## Summary
This proposal brings Durable Objects into Cloesce as first-class citizens. DOs open up a whole class of use cases that D1 just can't reach on its own: per-object isolated SQLite databases, strongly consistent state, and real-time WebSocket connections.

The gist: a new durable binding in the schema, the ability to back a Model with a DO, injecting the DO instance into API methods and utilizing its context, automatic Worker-to-DO request forwarding for API methods, and generation for both Wrangler and SQL migrations.

---

## Motivation

Durable Objects are easily the most powerful product on the Cloudflare Workers platform, but they are also the hardest to use correctly. Without framework support, a developer must manually wire up routing, state initialization, migrations, and the Worker-to-DO request handoff, all before writing any application logic. This proposal aims to eliminate that boilerplate by integrating Durable Objects as first-class citizens in Cloesce's schema and code generation.

---

## Goals and Non-Goals

### Goals

- Define Durable Object bindings
- Generate a Wrangler Configuration for each Durable Object binding
- Back a Model against a Durable Object
- Generate migrations for Durable Object backed Models
- Execute API methods in a Durable Object


### Non-Goals

- WebSocket RPC generation

---

## Design

### Durable Object Bindings

In order to introduce Durable Objects to a Cloesce schema, you first need to define some Durable Object binding. This will be done via the `durable` keyword:

```cloesce
durable LeaderboardDo {
    shard {
        tenantId: int
    }

    kv topEntryCache() -> array<LeaderboardEntry> {
        "top"
    }
}
```

#### Sharding

Any number of shards (or instances) of a Durable Object can be created from some Durable Object binding. Cloesce requires the method of locating/creating a shard to be explicitly stated in the schema: "`LeaderboardDo` is sharded by its `tenantId`". Multiple shards can be used, and they can be of any non-object serializable type.

If a binding is declared without a `shard` block, it is considered to be "global", meaning it has only a single instance and it is not sharded.

Cloesce will take an opinionated approach to creating the seed for the Durable Object's ID, always following the form:

```
{BINDING_NAME}/{SHARD_VALUE_1}/{SHARD_VALUE_2}/.../{SHARD_VALUE_N}
```

where the order of the shard values is determined by the order of the shard fields in the schema.

> [!NOTE]
> Cloesce will not provide a way to migrate from one set of shard fields to another.

#### Storage Templates

Just like with KV and R2 binding templates, a Durable Object binding can declare some storage templates to be fetched from the Durable Object's storage. 

These are declared as methods on the Durable Object binding, and they can be used in any Model **backed by that Durable Object** (Models with different backings CANNOT use these storage templates).

Storage templates follow the same syntax as KV templates, even using the same `kv` keyword. Templates will **not** be able to use `shard` fields, because there is no real use case (storage is already inside that specific DO).


#### Shard Field Validators

Shard fields can declare any number of validator tags:

```cloesce
durable LeaderboardDo {
    shard {
        [gt 0]
        tenantId: int
    }
}
```

These tags will be inherited by any Model that is backed by the Durable Object.

### Backing Models

If a Model is "backed by" a Durable Object, it is capable of hydrating itself from that Durable Object's KV and SQLite storage.

For example:

```cloesce
durable LeaderboardDo {
    shard {
        tenantId: int
    }

    kv topEntryCache() -> array<LeaderboardEntry> {
        "top"
    }
}

model Leaderboard for LeaderboardDo(shard) {
    kv LeaderboardDo::topEntryCache {
        top
    }
}
```

Here, `Leaderboard` is backed by `LeaderboardDo`, and it is able to use the `topEntryCache` storage template declared on `LeaderboardDo` as a field.

In order to declare that a Model is backed by a Durable Object, the shard fields of that Durable Object must be bound to that Model. In the above case, `LeaderboardDo::tenantId` is bound to `Leaderboard::shard`. This means that whenever a `Leaderboard` instance is hydrated, it will use the value of `tenantId` from its shard to know which DO instance to hydrate from, aliased as `shard` in the Model.

#### SQLite Backing

A Model backed by a DO can also represent a table in that DO's SQLite storage, with the same patterns as D1 backing:

```cloesce
// ...using the previous `LeaderboardDo` and `Leaderboard` declarations...

model LeaderboardEntry for LeaderboardDo(tenantId) {
    primary {
        id: int
    }

    column {
        playerName: string
        score: int
    }
}
```

Since `LeaderboardEntry` is backed by the same DO as `Leaderboard`, it can be hydrated with the same shard field, and thus have access to the same DO instance and its SQLite storage. `LeaderboardEntry` declares a primary key and some columns, so it will be able to use the generated SQLite access methods to interact with that table in the DO's SQLite database.

#### Relationships between DO-Backed Models

When a Model is backed by a DO and it has a relationship to another Model (via the `nav` keyword), that other Model must be backed by the same DO.

Cloesce will then assume that the related Model exists in the same shard as the original Model, and will access that original Model's storage to hydrate the related Model. For example:

```cloesce
// ...using the previous `LeaderboardDo`

model Leaderboard for LeaderboardDo(tenantId) {
    primary {
        id: int
    }

    nav LeaderboardEntry::leaderboardId {
        entries
    }
}

model LeaderboardEntry for LeaderboardDo(tenantId) {
    primary {
        id: int
    }

    column {
        playerName: string
        score: int
    }

    foreign Leaderboard::id {
        leaderboardId
    }
}
```

If two shards of `LeaderboardDo` exist with `tenantId` 1 and 2, and we hydrate a `Leaderboard` with `tenantId` 1, Cloesce will only find `LeaderboardEntry` instances that are in `tenantId` 1's shard, and it will not find any `LeaderboardEntry` instances that are in `tenantId` 2's shard.

#### Inheriting Shard Fields

A shard field declared on a DO is directly placed on any Model that is backed by that DO. For example, the `Leaderboard` Model in the above example would serialize to:

```json
{
    "id": 123,
    "tenantId": 1,
    "entries": [
        // ...
    ]
}
```

This allows Cloesce to fulfill the RPC contract of having all the information needed to route to the correct DO instance in the API method parameters, without requiring the API method to have any DO-specific parameters.

All validators of the DO's shard fields will also be inherited by the Model.

### APIs and Execution Context

Cloesce currently executes all API methods within the Worker where the `CloesceApp` is invoked. However, DOs have their own execution context, and in order to interact with their storage, API methods need to be executed within that context.

We will allow any API method of any Model to be executed in the context of a DO via `inject` blocks:

```cloesce
durable LeaderboardDo {
    shard {
        tenantId: int
    }

    kv topEntryCache() -> array<LeaderboardEntry> {
        "top"
    }
}

model Leaderboard for LeaderboardDo(shard) {
    kv LeaderboardDo::topEntryCache {
        top
    }
}

api Leaderboard {
    // `topScores` injects `LeaderboardDo` and instantiates it,
    // thus `topScores` is executed INSIDE a DO
    //
    // static method, executes IN a DO
    get topScores(tenantId: int) -> array<LeaderboardEntry> {
        inject {
            LeaderboardDo(tenantId)
        }
    }

    // `allLeaderboards` injects LeaderboardDo WITHOUT instantiating it,
    // meaning it just gets a DurableObjectNamespace<LeaderboardDo>.
    //
    // The method is static, executes IN a WORKER
    get allLeaderboards() -> array<Leaderboard> {
        inject {
            LeaderboardDo
        }
    }

    // `postScore` takes the `self` keyword, meaning it by default will have an
    // instantiated `LeaderboardDo` from `Leaderboard::shard` 
    //
    // The method is instantiated, executes IN a DO
    post postScore(self) {}
}

model HasNoBacking {}
api HasNoBacking {

    // Even though `HasNoBacking` is not backed by `LeaderboardDo`,
    // it can still inject and thus be executed inside of `LeaderboardDo`
    //
    // Static method, executes IN a DO
    get leaderboard(tenantId: int) {
        inject {
            LeaderboardDo(tenantId)
        }
    }
}
```

In the above code, several different execution contexts are demonstrated.

- `topScores` is a static method of `Leaderboard`, but it injects an instantiated `LeaderboardDo`, so it will be executed inside of a DO.

- `allLeaderboards` is a static method of `Leaderboard`, and it injects the `LeaderboardDo` binding without instantiating it, so it will be executed in the Worker.

- `postScore` is an instance method of `Leaderboard`, and since `Leaderboard` is backed by `LeaderboardDo`, it will have an instantiated `LeaderboardDo` injected by default, so it will be executed inside of a DO.

- `leaderboard` is a static method of `HasNoBacking`, but it injects an instantiated `LeaderboardDo`, so it will be executed inside of a DO, even though `HasNoBacking` itself is not backed by that DO.

#### Semantic Errors

1. A method that injects a DO binding with shard fields must provide arguments for those shard fields of the correct type.
2. Arguments provided for shard fields must exist as API method parameters
3. Multiple DO instantiations in the same inject block are not allowed.
4. An instantiated method backed by a DO cannot inject any DO, as it implicitly injects the DO it is backed by (rule 3).

#### Inheriting Validators

In the case:

```cloesce
durable LeaderboardDo {
    shard {
        [gt 0]
        tenantId: int
    }
}

// ...
api Leaderboard {
    get leaderboard(tenantId: int) {
        inject {
            LeaderboardDo(tenantId)
        }
    }
}
```

`LeaderboardDo::tenantId` has a validator that states it must be greater than 0. Since `tenantId` is an argument to the injected `LeaderboardDo`, the same validator will be applied to the `tenantId` parameter of the `leaderboard` method.

This allows validators to be declared once on shard fields, and then be inherited by any API method that injects that DO with those shard fields as arguments.

### Migrations

A Durable Object can be migrated in two ways: changes to the DO class's underlying Wrangler configuration, and changes to the DO's SQLite schema.

#### SQL

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

#### Wrangler

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

## Implementation

Implementation can be broken into several phases:

1. Durable Object Bindings
2. API Method Execution in Durable Objects
3. Backing Models with Durable Objects 
4. Migrations
5. Instantiated API Methods in Durable Objects

### Durable Object Bindings

This phase will focus on adding the Durable Object binding syntax to the schema, and generating the corresponding Wrangler configuration and backend class for each Durable Object binding.

**Generated Wrangler Config**:
- For each Durable Object binding, a Wrangler configuration will be generated with a `durable_objects` entry for that DO (JSON + TOML support)

**Backend Class**:
- Generate a TypeScript backend class for each DO binding
- Add static properties to access storage templates (key retrieval and value retrieval)
- Add a static `Shard` property with accessors for the template, DO ID, and DO instance for the DO's shard fields.

#### Generated Code

Each DO will translate to a Wrangler Config such as:

```toml
[[durable_objects]]
name = "LeaderboardDo"
class_name = "LeaderboardDo"
```

On the backend, a class will be generated for each DO:

```ts
export abstract class LeaderboardDo implements DurableObject {
    static readonly Shard = {
        template: (tenantId: int) => `LeaderboardDo/${tenantId}`,
        id: (tenantId: int, env: Env) => env.LeaderboardDo.idFromName(LeaderboardDo.Shard.template(tenantId)),
        instance: async (tenantId: int, namespace: DurableObjectNamespace<LeaderboardDo>) 
            => namespace.get(LeaderboardDo.Shard.id(tenantId, env))
    }

    static readonly topEntryCache = {
        template: () => "top",
        value: (storage: DurableObjectStorage) => storage.get("top")
    }

    // or if some arguments were needed:
    static readonly topEntryCacheWithDate = {
        template: (date: string) => `top/${date}`,
        value: (storage: DurableObjectStorage, date: string) => storage.get(`top/${date}`)
    }
}
```

### API Method Execution in Durable Objects

This phase will add the syntax for executing API methods in the context of a DO, via `inject` blocks (removing the old `inject` tag syntax). Then, it will add semantic analysis for the rules around DO injection and execution contexts. Finally, it will be responsible for ensuring the runtime is actually capable of executing API methods in a DO context, with the correct injected DO instances.

#### Augmenting the Cloesce Router

In order for an API method to be executed in a DO, it must be forwarded to that DO instance from a Worker.

The lifetime of a request for a static invocation from the Worker through the Cloesce Router can be modeled by these states:
1. Client sends request to Worker (GET /api/Person/speak)
2. Worker invokes the Cloesce Router
3. Router matches the request to `Person::speak`
4. Router validates the request parameters against `Person::speak`
5. Router dispatches the request to the `Person::speak` method implementation
6. Return result response to client

An instance Model method invocation would have the extra step of hydration before step 5, but the rest of the flow would be the same.

In order to support DO execution contexts, we will need to augment the flow to include a DO forwarding step. The new flow would look like this:

1. Client sends request to Worker (GET /api/Leaderboard/topScores?tenantId=1)
2. Worker invokes the Cloesce Router
3. Router matches the request to `LeaderboardDo::topScores`
4. Router validates the request parameters against `LeaderboardDo::topScores`
5. Router forwards to `LeaderboardDo(tenantId)` instance
6. The DO instance invokes its own Cloesce Router with the same request
7. Router matches the request to `LeaderboardDo::topScores`
8. Router dispatches the request to the `LeaderboardDo::topScores` method implementation

Note that in this sequence, the DO's Cloesce Router will skip the parameter validation step, since the Worker will have already validated the parameters before forwarding the request to the DO. The DO's Router will only be responsible for matching the request to the correct API method and dispatching it.

If the API method is an instance method, the DO's Router will also be responsible for hydrating the instance before dispatching the request to the method implementation (discussed further in the next phase).

An implementation for the `LeaderboardDo` class would look like this:

```ts
// main.ts
const Leaderboard = clo.Leaderboard.impl({
    // types can be omitted but are included here for clarity
    async topScores(tenantId: int, env: { LeaderboardDo: clo.LeaderboardDo }) {
        // Since there is an instance of `LeaderboardDo`, we must be inside of the DO's execution context,
        // and can access the DO's storage templates and shard fields directly.
    }
})

export class LeaderboardDo extends clo.LeaderboardDo {
    constructor (state: DurableObjectState, env: clo.Env) {
        super(state, env);

        state.blockConcurrencyWhile(async () => {
            this.app = await super.cloesce(env);
            this.app.register(Leaderboard);
        });
    }

    async fetch(request: Request) {
        // Developer can do any pre/post-processing here
        return await this.app.run(request)
    }
}
```

Each DO's backing class will be given a `cloesce(env)` method which creates a `CloesceApp` marked explicitly to **not forward requests**, such that we cannot accidentally create an infinite forwarding loop.

### Backing Models with Durable Objects

This phase will add the syntax for backing a Model with a Durable Object, with the ability to use any KV storage templates declared on that DO as fields in the Model. Additionally, it will add semantic analysis for the shard field inheritance, DO-specific storage template usage, and rules with `self` and `inject` in API methods of DO-backed Models.

Code generation of Data Sources and CRUD methods, along with runtime support will be added in a later phase.

### Migrations

This phase will cover the generation of migration files for DO-backed Models, and the necessary runtime support for applying those migrations to the DO's SQLite database. Additionally, it will cover the generation of Wrangler configuration migrations for changes to DO bindings.

### Instantiated API Methods in Durable Objects

Finally, this phase will cover the ability for instance methods of DO-backed Models to be executed in the context of a DO, by adding:

- `CRUD` Data Source method generation
- Automatic injection of the DO instance for API methods of DO-backed Models, via the `self` keyword
- Runtime support for hydrating DO-backed Model instances in the DO's execution context, with the correct injected DO instance and access to that DO's storage templates.