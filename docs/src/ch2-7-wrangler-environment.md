# Wrangler Environment

> *Note*: Unlike some Infrastructure as Code tools for Cloudflare, Cloesce does not try to fully replace using a Wrangler configuration file. Account specific configuration settings such as binding IDs must still be managed manually.

Cloesce will search your project for a class decorated with `@WranglerEnv` to define the [Cloudflare Workers environment](https://developers.cloudflare.com/workers/configuration/environment-variables/) tailored to your application. 

A `wrangler.toml` file is generated during compilation based on this class. Currently, only D1 databases, R2 buckets, KV namespaces, and string environment variables are supported.

Cloesce will not overwrite an existing `wrangler.toml` file or any unique configurations you may have added to it. It will append any missing bindings and variables defined in the `@WranglerEnv` class.

An instance of the `WranglerEnv` is always available as a dependency to inject. See the [Services](./ch3-0-services.md) chapter for more information on dependency injection.

An example `WranglerEnv` class is shown below:

```typescript
@WranglerEnv
export class Env {
    db: D1Database;
    bucket: R2Bucket;
    kv: KVNamespace;
    someVariable: string;
}
```

This will compile to the following `wrangler.toml` file:
```toml
compatibility_date = "2025-10-02"
main = ".generated/workers.ts"
name = "cloesce"

[[d1_databases]]
binding = "db"
database_id = "replace_with_db_id"
database_name = "replace_with_db_name"

[[r2_buckets]]
binding = "bucket"
bucket_name = "replace-with-r2-bucket-name"

[[kv_namespaces]]
binding = "kv"
namespace_id = "replace_with_kv_namespace_id"

[vars]
someVariable = "default_string"
```

> *Alpha Note*: Only one D1 database binding is currently supported. Future releases will allow multiple D1 bindings.