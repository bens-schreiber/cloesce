# Durable Objects

[Cloudflare Durable Objects](https://developers.cloudflare.com/durable-objects/) provide a way to run stateful code on Cloudflare's edge network.

To describe them simply (_a task difficult to do justice_), Durable Objects are:

1. A place to store data (SQLite and KV storage).
2. A single threaded sequential execution context.
3. Capable of being sharded across any number of instances (think _database-per-X_).

Cloesce provides first class support for Durable Objects:

- Define DOs in your schema
- Generate a fully typed interface
- Use them as a [Model backing](./ch4-1-sqlite-backed-model.md#with-durable-objects)
- Execute API methods [within a DOs context](./ch6-1-rest-apis.md#execution-context)

> [!WARNING]
> Cloesce is only capable of using the modern [SQLite backed Durable Objects](https://developers.cloudflare.com/durable-objects/best-practices/access-durable-objects-storage/#sqlite-storage-backend), and does not support the legacy Durable Object storage API.

## Defining a Durable Object Binding

To define a Durable Object environment binding, use the `durable` block:

```cloesce
durable MyShardedDo {
    shard {
        tenant: int
    }

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

- `MyShardedDo`: Any number of Durable Object instances can be created with different shard parameters. In this case, the `tenant` parameter is used to shard the Durable Object by tenant ID.

- `MyGlobalDo`: A singleton Durable Object that will always route to the same instance.

In both bindings, KV templates can be defined to generate a typed interface for interacting with the Durable Object's KV storage.

### Extending the Durable Object Class

The Cloesce Router will forward HTTP requests bound for a particular Durable Object from the Worker to the `fetch` method of the generated Durable Object class.

To implement custom logic for handling these requests, extend the generated Durable Object class and implement the `fetch` method:

```ts
import { createApp, ... } from "@cloesce/backend";

export class SubRedditDo extends DurableObject<CfEnv> {
  private base = createApp()
    .durable(this, [...migrations]);

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
