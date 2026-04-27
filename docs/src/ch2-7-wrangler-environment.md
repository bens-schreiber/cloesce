# Wrangler Environment

> [!NOTE]
> Unlike some Infrastructure as Code tools for Cloudflare, Cloesce does not try to fully replace using a Wrangler configuration file. Account specific configuration settings such as binding IDs must still be managed manually.

Cloesce requires an `env` to be defined in any schema that uses D1, KV, or R2 bindings, which is used to define the [Cloudflare Workers environment](https://developers.cloudflare.com/workers/configuration/environment-variables/) tailored to your application.

A `wrangler.jsonc` or `wrangler.toml` file is generated during compilation based on the `env` block. Configure your preference in the `cloesce.jsonc` file:

```jsonc
{
  "src_paths": ["./src/schema"],
  "workers_url": "http://localhost:5000/api",
  "wrangler_config_format": "jsonc", // or "toml"
}
```

Currently, only D1 databases, R2 buckets, KV namespaces, and string environment variables are supported.

Cloesce will not overwrite an existing wrangler file or any unique configurations you may have added to it. It will append any missing bindings and variables defined in the `env` block.

An instance of the Wrangler environment is always available as a dependency to inject. See the [Services](./ch3-0-services.md) chapter for more information on dependency injection.

An example Wrangler environment is shown below:

```cloesce
env {
    d1 {
        db
        // ...
    }

    kv {
        kv
        // ...
    }

    r2 {
        bucket
        // ...
    }

    vars {
        someVariable: string
        otherVariable: int
    }
}

```

This will compile to the following `wrangler.jsonc` file:

```jsonc
{
  "compatibility_date": "2025-10-02",
  "main": ".generated/workers.ts",
  "name": "cloesce",
  "d1_databases": [
    {
      "binding": "db",
      "database_id": "replace_with_db_id",
      "database_name": "replace_with_db_name",
      "migrations_dir": "./migrations/db",
    },
  ],
  "r2_buckets": [
    {
      "binding": "bucket",
      "bucket_name": "replace-with-r2-bucket-name",
    },
  ],
  "kv_namespaces": [
    {
      "binding": "kv",
      "namespace_id": "replace_with_kv_namespace_id",
    },
  ],
  "vars": {
    "someVariable": "default_string",
    "otherVariable": 0,
  },
}
```
