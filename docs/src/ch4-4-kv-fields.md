# KV Fields

Any Model may have any number of [Cloudflare KV](https://developers.cloudflare.com/kv/) hydrated fields.

KV fields reference templates defined in a [KV bindings](./ch3-1-kv-and-r2.md#workers-kv) or [Durable Object bindings](./ch3-3-durable-objects.md).

## Defining a KV Field

A field in a Model can be hydrated from KV by referencing a binding defined on a `kv` namespace:

```cloesce
kv MyNamespace {
    settings -> json { }
}

model User {
    kv MyNamespace::settings {
        settings
    }
}
```

The above snippet defines a Model `User` with a KV field `settings` that is sourced from the namespace `MyNamespace` under the static key `"settings"`.

The value in the template is typed as `json`, and Cloesce will automatically handle the serialization and deserialization of this field when reading from and writing to KV.

> [!NOTE]
> If a Model wants to use a Durable Objects KV field, shard fields must be provided in `kv` field:
>
> ```cloesce
> durable MyDurableObject {
>     shard {
>         tenant: string
>     }
>
>     settings -> json { }
> }
>
> model User for MyDurableObject::tenant {
>     kv MyDurableObject::{settings, tenant} {
>         settings
>     }
> }
> ```

## Key Interpolation

A common pattern is to format a key such that any number of related values can be stored under that template. For example:

```cloesce
kv MyNamespace {
    profile -> json {
        userId: string

        "profile/{userId}"
    }

    profileImplicitKey -> json {
        userId: string
    }
}
```

Here, `profile` accepts one parameter `userId`, which is a string. The key for this field in KV is defined as `"profile/{userId}"`, where `{userId}` is a placeholder that will be replaced with the actual value of the `userId` parameter when accessing KV.

In the `profileImplicitKey` field, the key is not explicitly defined, so Cloesce will automatically generate a key based on the field name and its parameters. In this case, the key will be `"profileImplicitKey/userId/{userId}"`.

Any `column` or `route` field on a Model can be used to populate the parameters of a KV field, as long as the types match. For example:

```cloesce
model User for MyDb {
    primary {
        id: int
    }

    column {
        name: string
    }

    kv MyNamespace::profile(id) {
        profile
    }
}
```
