# Workers KV and R2

> [!TIP]
> KV and R2 binding templates can be referenced on any Model via [KV Fields](./ch4-3-kv-fields.md) and [R2 Fields](./ch4-4-r2-fields.md), allowing you to easily integrate Workers KV and R2 across the full stack of the application.

> [!NOTE]
> The Cloesce Compiler will not allow a binding template key format to collide with any other template in the binding.
>
> For example, if some binding were to have a key `foo/{bar}`, then no template would be allowed to have the leading prefix `foo/`.
> This allows the `list` prefix matching functionality of Workers KV and R2 to work without ambiguity.

> [!NOTE]
> KV definitions in the schema do not yet support cache control directives and expiration times. This is planned for a future release.

## Workers KV

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value store. Cloesce provides first class support for KV, allowing a simple binding declaration to generate not only a Wrangler configuration for the namespace, but also a fully typed interface for querying that namespace in your application code.

Cloudflare KV has a simple toolset and API for storing and retrieving key-value pairs. Specifically, KV supports:

- Eventually consistent writes
- List queries with key based prefix matching and pagination
- Metadata on each key-value pair
- Maximum 25MB value size limit

To define a Workers KV namespace binding, use the `kv` block:

```cloesce
kv MyNamespace {
    // ... define as many binding templates as necessary
    settings() -> json {
        "path/to/settings"
    }

    // accept any number of parameters and use them
    // in the template string to generate dynamic keys
    session(token: string) -> SessionToken {
        "sessions/{token}"
    }
}
```

In the above example `MyNamespace` is the name of a KV namespace, and `settings` and `session` are "binding templates" defined on that namespace.

Every binding template describes a location of data in the KV namespace, and the return type of the template describes the expected type of that data. The template string can be generated from any number of parameters, allowing for dynamic keys.

This definition will compile to an interface capable of querying the KV namespace with the defined key templates. For example:

```ts
settings: {
  template: () => `path/to/settings`,

  get: () => namespace.get(`path/to/settings`),

  put: (value) => namespace.put(`path/to/settings`, value),

  list: (options: { limit, cursor}) =>
    namespace.list({ ...options, prefix: `path/to/settings` }),
},

session: {
  template: (token) => `sessions/${token}`,

  get: (token) => namespace.get(`sessions/${token}`),

  put: (token, value) =>
    namespace.put(`sessions/${token}`, value),

  list: (options: { limit, cursor }) =>
    namespace.list({ ...options, prefix: `sessions/` }),
},

// ...
```

These methods will be merged on top of the Cloudflare `KVNamespace` interface in the Cloudflare Environment.

In addition to the backend interface, a Wrangler configuration will be generated:

```toml
[[kv_namespaces]]
binding = "MyNamespace"
id = "replace_with_my_namespace_id"
```

## R2

> [!NOTE]
> R2 is used to store large unstructured data. For this reason, Cloesce will not query and buffer the full value of an R2 field into the Worker runtime.
>
> Instead, only a `HEAD` request is made to R2 to check for existence and retrieve metadata.

To define a Cloudflare R2 bucket binding, use the `r2` block:

```cloesce
r2 MyBucket {
    // ... define as many binding templates as necessary
    getObject(key: string) {
        `path/to/${key}`
    }
}
```

Unlike [Workers KV](#workers-kv), R2 bindings do not have a return type, because they will always return the Cloudflare [R2Object](./ch2-0-type-reference.md#primitives) (a `HEAD` request to the object, not the full value).

A similar interface to the above `MyBucket` definition will be generated for querying the R2 bucket and accessing objects with the defined key templates.

In addition to the backend interface, a Wrangler configuration will be generated:

```toml
[[r2_buckets]]
binding = "MyBucket"
bucket_name = "replace-with-my_bucket-name"
```
