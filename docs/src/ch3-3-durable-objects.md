# Durable Objects

[Cloudflare Durable Objects](https://developers.cloudflare.com/durable-objects/) provide a way to run stateful code on Cloudflare's edge network.

To describe them simply (_a task which is difficult to do justice_), Durable Objects are:

1. A place to store data (SQLite and KV storage).
2. A single threaded sequential execution context.
3. Capable of being sharded across any number of instances (think _database-per-X_).

Cloesce provides first class support for Durable Objects:

- Define DOs in your schema
- Generate a fully typed interface
- Use them as a [Model backing](./ch4-1-sqlite-backed-model.md#with-durable-objects)
- Execute API methods [within a DOs context](./ch6-1-rest-apis.md#execution-context)

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
    settings -> json { }

    userMap -> json {
        userId: int
        "user/{userId}"
    }
}

durable MyGlobalDo {
    settings -> json { }
}
```

The above example defines two Durable Object bindings:

- `MyShardedDo`: A sharded Durable Object, meaning any number of Durable Object instances can be created with different shard parameters. In this case, the `tenant` parameter is used to shard the Durable Object by tenant ID.

- `MyGlobalDo`: A global Durable Object, meaning Cloesce will treat it as if there is only one instance of the Durable Object, and will not allow any shard parameters to be defined.

In both bindings, KV templates can be defined to generate a typed interface for interacting with the Durable Object's KV storage.

### Extending the Durable Object Class

The Cloesce Router will forward HTTP requests bound for a particular Durable Object from the Worker to the `fetch` method of the generated Durable Object class.

To implement custom logic for handling these requests, extend the generated Durable Object class and implement the `fetch` method:

```ts
import { createApp, ... } from "@cloesce/backend";

export class SubRedditDo extends DurableObject<CfEnv> {
  private base = createApp(this, SubRedditDoHost, [subRedditDoInitial])
    .register(SubReddit, subReddit)
    .register(Post, post);

  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}
```

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

## Backing Models

A Model may be backed by a Durable Object, meaning the data for that Model can be pulled from an instance of a Durable Object. Declarations must explicitly state each shard field on the Model (aliased to any name in shard field order):

```cloesce
durable MyShardedDo {
    shard {
        tenant: int
        org: string
    }
}

model Tenant for MyShardedDo::{tenant, org} {
    // ...
}

// can alias `tenant` to anything
model Tenant for MyShardedDo::{tenant(alias), org} {
    // ...
}

// if only one shard field, can use singular form
model Tenant for MyShardedDo::tenant(alias) {
    // ...
}
```

Read more about backing Models with Durable Objects in the [Models chapter](./ch4-1-sqlite-backed-model.md#with-durable-objects).

## Execution Context

Durable Objects provide a single threaded execution context in which their internal storage may be accessed and mutated.

Just because a Model is backed by a Durable Object does not mean that all [API methods](./ch6-1-rest-apis.md) and [Data Sources](./ch5-1-data-sources.md) that interact with that Model are executed within the Durable Object's execution context.

Explicitly [inject](./ch6-1-rest-apis.md#execution-context) the Durable Object's execution context into any API method or Data Source method to interact with the Durable Object's internal storage. For example:

```cloesce
api AnyModel {
    get doSomething -> json {
        tenant: int

        inject { MyShardedDo::tenant(tenant) }
        // or short form: inject { MyShardedDo::tenant }
    }
}
```

In order to instantiate the `MyShardedDo` execution context, the `tenant` shard parameter must be passed in as an argument to the API method under an `inject` block.

Read more about execution context injection in the [API chapter](./ch6-1-rest-apis.md#execution-context) and the [Data Source chapter](./ch5-1-data-sources.md#execution-context).
