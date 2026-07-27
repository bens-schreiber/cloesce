# Workers KV and R2

> [!NOTE]
> Cache control directives and expiration times are planned for a future release.

## Workers KV

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value store.

Define a KV binding in your schema to generate a matching Wrangler configuration and a fully a fully typed interface for [querying that namespace in your application code](./ch7-0-orm-reference.md).

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

A Wrangler configuration will be generated:

```toml
[[kv_namespaces]]
binding = "MyNamespace"
namespace_id = "replace-with-my_namespace-id"
```

## R2

Define a [Cloudflare R2](https://developers.cloudflare.com/r2/) binding in your schema to generate a matching Wrangler configuration and a fully typed interface for [querying that bucket in your application code](./ch7-0-orm-reference.md).

```cloesce
r2 MyBucket {
    getObject {}

    getObjectCustomKey {
        param1: string
        param2: string

        "path/to/{param1}/{param2}"
    }
}
```

Unlike [Workers KV](#workers-kv), R2 bindings do not have a return type, because they will always return the Cloudflare [R2Object](./ch2-0-type-reference.md#primitives) (a `HEAD` request to the object, not the full value).

Additionally, a Wrangler configuration will be generated:

```toml
[[r2_buckets]]
binding = "MyBucket"
bucket_name = "replace-with-my_bucket-name"
```
