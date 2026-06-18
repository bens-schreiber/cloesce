# Durable Objects

[Cloudflare Durable Objects](https://developers.cloudflare.com/durable-objects/) provide a way to run stateful code on Cloudflare's edge network.

To describe them simply (_a task which is difficult to do justice_), Durable Objects are:

1. A place to store data (SQLite and KV storage).
2. A single threaded sequential execution context.
3. Capable of being sharded across any number of instances (think _database-per-X_).

Cloesce provides first class support for Durable Objects, allowing you to define them in your schema, generate a fully typed interface, [injecting execution context into your API implementations](./ch6-1-rest-apis.md#execution-context), and using them as a [Model backing](./ch4-1-sqlite-backed-model.md#with-durable-objects).

> [!TIP]
> Durable Objects are **not** a Model, but rather a place that any number of Models can be backed by. 
> 
> A Durable Object instance can store any number of Models within its SQLite and KV storage, and can execute any code necessary to manage those Models. 
>
> For more on using Durable Objects as a Model backing, see the [Models chapter](./ch4-1-sqlite-backed-model.md#with-durable-objects).

> [!WARNING]
> Cloesce is only capable of using the modern [SQLite backed Durable Objects](https://developers.cloudflare.com/durable-objects/best-practices/access-durable-objects-storage/#sqlite-storage-backend), and does not support the legacy Durable Object storage API.

## Defining a Durable Object Binding

To define a Durable Object environment binding, use the `durable` block:

```cloesce
durable MyShardedDo {
    shard {
        tenant: int
    }

    // ... define as many binding templates as necessary
    settings() -> json {
        "settings"
    }

    userMap(userId: int) -> json {
        "user/{userId}"
    }
}

durable MyGlobalDo {
    settings() -> json {
        "settings"
    }
}
```

The above example defines two Durable Object bindings:

- `MyShardedDo`: A sharded Durable Object, meaning any number of Durable Object instances can be created with different shard parameters. In this case, the `tenant` parameter is used to shard the Durable Object by tenant ID.

- `MyGlobalDo`: A global Durable Object, meaning Cloesce will treat it as if there is only one instance of the Durable Object, and will not allow any shard parameters to be defined.

In both bindings, KV templates can be defined to generate a typed interface for interacting with the Durable Object's KV storage. 

In the case of `MyShardedDo`, the `userMap` template will generate an interface for storing user data in the Durable Object's KV storage, with keys formatted as `"user/{userId}"`.

### Generated Interface

Cloesce will create an abstract class to extend for each Durable Object binding defined in the schema, along with helper functions merged into the Cloesce `Env` type.

The generated abstract class provides:

- KV template accessors (e.g. `this.settings`, `this.userMap(userId)`) with `get`, `put`, `list`, and `template` methods for interacting with the Durable Object's KV storage.

- A `cloesce` method for applying [generated migrations](./ch1-3-building-and-migrating.md#migrations) as well as invoking the [Cloesce Router](./ch6-1-rest-apis.md).

The generated `Env` helpers provide:

- `env.MyShardedDo.template(tenant)` — returns the shard key string for a given set of shard parameters.
- `env.MyShardedDo.id(tenant)` — returns a `DurableObjectId` for the given shard parameters.
- `env.MyShardedDo.stub(tenant)` — returns a typed `DurableObjectStub` for the given shard parameters.

For global Durable Objects (no shard fields), these helpers take no arguments.

### Extending the Durable Object Class

The Cloesce Router will forward HTTP requests bound for a particular Durable Object from the Worker to the `fetch` method of the generated Durable Object class. 

To implement custom logic for handling these requests, extend the generated Durable Object class and implement the `fetch` method:

```ts
import * as clo from "@cloesce/backend.js";

export class MyShardedDo extends clo.MyShardedDo {
    app: CloesceApp;

    constructor(state: DurableObjectState, env: clo.CfEnv) {
        super(state, env);
        this.app = this.cloesce(env, [...migrations]);
        this.app.register(...);
    }

    async fetch(request: Request): Promise<Response> {
        return await this.app.run(request);
    }
}
```

Here, the `MyShardedDo` class extends the generated `clo.MyShardedDo` class, and implements the `fetch` method to handle incoming HTTP requests. 

The `cloesce` method is used to create a Cloesce application instance, which can be used to register API implementations and run the application.

### Wrangler Configuration

A Wrangler configuration will be generated for each Durable Object binding defined in the schema:

```toml
[[durable_objects.bindings]]
class_name = "MyShardedDo"
name = "MyShardedDo"

[[durable_objects.bindings]]
class_name = "MyGlobalDo"
name = "MyGlobalDo"

[[migrations]]
new_sqlite_classes = [
    "MyShardedDo",
    "MyGlobalDo",
]
tag = "v1"
```
