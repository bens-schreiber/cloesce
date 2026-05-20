# Environment Declaration

Environment bindings are a quick and easy way to declare, manage, reference and inject Cloudflare Workers bindings across your application.

Currently, Cloesce supports [D1](https://developers.cloudflare.com/d1/), [KV](https://developers.cloudflare.com/kv/), [R2](https://developers.cloudflare.com/r2/), and custom [Wrangler Environment Variables](https://developers.cloudflare.com/workers/configuration/environment-variables/).

## Schema Example

> [!TIP]
> Any top level declaration in Cloesce is global across any file in the project. This means that environment bindings declared in one file can be referenced and used in any other file.

To define environment bindings in your Cloesce schema, use the `env` block. Any number of `env` blocks can be defined in a schema, and they can be placed anywhere in the file.

```cloesce
env {
    d1 {
        my_db
        my_other_db
    }

    kv {
        my_namespace
    }

    r2 {
        my_bucket
    }

    vars {
        secret: string
        another_secret: int
    }
}
```

### Generated Wrangler Configuration

The above schema will generate the following Wrangler configuration file (_TOML format shown here, but JSON is also supported based on your `cloesce.jsonc` settings_):

```toml
[[d1_databases]]
binding = "my_db"
database_id = "replace_with_my_db_id"
database_name = "replace_with_my_db_name"
migrations_dir = "./migrations/my_db"

[[d1_databases]]
binding = "my_other_db"
database_id = "replace_with_my_other_db_id"
database_name = "replace_with_my_other_db_name"
migrations_dir = "./migrations/my_other_db"

[[kv_namespaces]]
binding = "my_namespace"
id = "replace_with_my_namespace_id"

[[r2_buckets]]
binding = "my_bucket"
bucket_name = "replace-with-my_bucket-name"

[vars]
secret = "default_string"
another_secret = 0
```

Note that the `database_id`, `database_name`, `migrations_dir`, `id`, and `bucket_name` fields in the generated configuration file are required for Wrangler to recognize the bindings, but they are not defined in the Cloesce schema. You will need to fill in these fields manually after compilation.

In future releases, we plan to rely solely on the Cloesce schema for environment declaration, and generate the necessary configuration for Wrangler without any manual intervention. Stay tuned!
