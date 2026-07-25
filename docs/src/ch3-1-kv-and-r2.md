# Workers KV and R2

> [!TIP]
> KV and R2 binding templates can be referenced on any Model via [KV Fields](./ch4-3-kv-fields.md) and [R2 Fields](./ch4-4-r2-fields.md), allowing you to easily integrate Workers KV and R2 across the full stack of the application.

> [!NOTE]
> KV definitions in the schema do not yet support cache control directives and expiration times. This is planned for a future release.

## Workers KV

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value store. Cloesce provides first class support for KV, allowing a simple binding declaration to generate not only a Wrangler configuration for the namespace, but also a fully typed interface for querying that namespace in your application code.

KV supports:

- Eventually consistent writes
- List queries with key based prefix matching and pagination
- Metadata on each key-value pair
- Maximum 25MB value size limit

To define a Workers KV binding, use the `kv` block:

```cloesce
kv MyNamespace {
    settings -> json {}

    // accept any number of parameters
    session -> SessionToken {
        token: string
    }

    // write custom key templates
    custom -> string {
        param1: string
        param2: string

        "path/to/{param1}/{param2}"
    }
}
```

A full interface to query the stores defined templates will be generated for your application code.

Additionally, a Wrangler configuration will be generated:

```toml
[[kv_namespaces]]
binding = "MyNamespace"
namespace_id = "replace-with-my_namespace-id"
```

## R2

```cloesce
r2 MyBucket {
    getObject {}
}
```

Unlike [Workers KV](#workers-kv), R2 bindings do not have a return type, because they will always return the Cloudflare [R2Object](./ch2-0-type-reference.md#primitives) (a `HEAD` request to the object, not the full value).

A full interface to query the stores defined templates will be generated for your application code.

Additionally, a Wrangler configuration will be generated:

```toml
[[r2_buckets]]
binding = "MyBucket"
bucket_name = "replace-with-my_bucket-name"
```
