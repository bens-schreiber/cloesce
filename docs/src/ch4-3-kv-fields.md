# KV Fields

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value store. Cloesce provides first class support for KV, allowing you to define Models with KV-hydrated fields that seamlessly integrate with [D1](./ch4-1-sqlite-backed-model.md) backed Models and [R2](./ch4-4-r2-fields.md) fields.

## Defining an Environment Binding

To use KV fields in your Models, you first need to define an environment binding for the KV namespace in your Cloesce schema:

```cloesce
kv MyNamespace {
    // ...templates
    settings() -> json {
        "settings"
    }

    profile(userId: string) -> json {
        "profile/{userId}"
    }
}
```

Additionally, a [Durable Object binding](./ch3-3-durable-objects.md) can also act as a KV namespace:

```cloesce
durable MyDurableObject {
    settings() -> json {
        "settings"
    }

    profile(userId: string) -> json {
        "profile/{userId}"
    }
}
```

> [!NOTE]
> Durable Object storage is only accessible from within the context of that Durable Object. 
>
> If want to use a Durable Object as a KV namespace for some Model, that Model must be backed by that Durable Object
> using `for`, e.g. `model User for MyDurableObject`. 
>
> You will not be able to use a Durable Object as a KV namespace for a Model that is backed by D1 or has no backing at all.

Read more about [KV bindings](./ch3-1-kv-and-r2.md#workers-kv) and [Durable Object bindings](./ch3-3-durable-objects.md) in the Environment chapter.

## Defining a KV Field

> [!NOTE]
> Models are not storage-monolithic. A Model that uses SQLite fields can also have KV fields, and R2 fields, all at the same time.

A field in a Model can be hydrated from KV by referencing a binding defined on a `kv` namespace:

```cloesce
kv MyNamespace {
    settings() -> json {
        "settings"
    }
}

model User {
    kv MyNamespace::settings() {
        settings
    }
}
```

The above snippet defines a Model `User` with a KV field `settings` that is stored in the namespace `MyNamespace` under the static key "settings". 

`settings` is typed as `json`, and Cloesce will automatically handle the serialization and deserialization of this field when reading from and writing to KV.

### Quirks of a non-SQLite backed Model

Unlike a [SQLite backed Model](./ch4-1-sqlite-backed-model.md), the above `User` Model does not define a backing database with `for`. 

The `settings` field is therefore not associated with any particular row, and the Model has no underlying "backing": Cloesce simply hydrates `settings` from KV whenever you query for a `User`.

> [!NOTE]
> If you query for a `User` and the `settings` key does not exist in KV, Cloesce returns `null` for the field rather than a `404` error (as it would for a SQLite backed Model with a missing row). Cloesce cannot distinguish an absent key from a key that exists with a `null` value.

## Key Interpolation

A common pattern is to format a key such that any number of related values can be stored under that template. For example:

```cloesce
kv MyNamespace {
    profile(userId: string) -> json {
        "profile/{userId}"
    }
}
```

Here, `profile` accepts one parameter `userId`, which is a string. The key for this field in KV is defined as `"profile/{userId}"`, where `{userId}` is a placeholder that will be replaced with the actual value of the `userId` parameter when accessing KV.

There are two ways to pass the value for `userId` when querying for this field:

### Route Fields

> [!NOTE]
> Route fields and SQLite columns are mutually exclusive. If a Model defines any SQLite columns, it cannot use route fields, and vice versa.
>
> Notably, a Durable Object backed Model _can_ use route fields, _iff_ it does not define any SQLite columns. However, a D1 backed Model cannot use route fields at all, as D1 backing requires the presence of SQLite columns.

> [!NOTE]
> A Model that is not backed by any database is commonly referred to as "Worker Backed", because its data is not persisted in any database and only exists in memory during the execution of a Worker.

> [!NOTE]
> `route` fields are limited to [SQLite compatible types](./ch2-0-type-reference.md#sqlite-compatible-types)

If a Model aims to exist without any SQLite backing at all, it may use `route` fields to populate the parameters for its KV fields:

```cloesce
model User {
    route {
        userId: string
    }

    kv MyNamespace::profile(userId) {
        profile
    }
}
```

The `route` block defines a `userId` field that is populated from the URL route when querying for a `User`. For example, if you query for `/users/123`, Cloesce will populate `userId` with the value "123", and then use that value to construct the key for the KV field `profile`, finally fetching the value from KV.

### Columns

If a Model is SQLite backed, it can use the values of its columns to populate the parameters for its KV fields:

```cloesce
model User for MyDb {
    primary {
        id: int
    }

    kv MyNamespace::profile(id) {
        profile
    }
}
```

The Cloesce ORM will know to first hydrate the table `User` to get the value of `id`, and then use that value to construct the key for the KV field `profile`, finally fetching the value from KV.

## Key Template Conventions

KV is capable of being queried by a prefix, listing all keys that exist under it. Cloesce will enforce that the schema does not overlap keys in a way that would make results ambiguous. For example, the following schema would be invalid:

```cloesce
kv MyNamespace {
    profile(userId: string) -> json {
        "profile/{userId}"
    }

    favoriteNumber(userId: string) -> int {
        "profile/{userId}/favNum"
    }
}
```

In this example, the template
- `"profile/{userId}/favNum"` 

overlaps with
- `"profile/{userId}"` 

A prefix list on `"profile/"` would include `"profile/{userId}/favNum"` because it matches the prefix, even though it does not conform to the `profile` template. 

Cloesce throws an error when validating this schema, preventing such ambiguities.

## Generated Code

### KValue

When pulling from a KV namespace (_not_ Durable Object storage), Cloesce will return an instance of the `KValue` class for that field:

```ts
// both .cloesce/client.ts and .cloesce/backend.ts
export class KValue<V> {
  raw: unknown | null;
  metadata: unknown | null;

  get value(): V | null {
    return this.raw as V | null;
  }
}
```

Cloesce will make **no effort** to validate that the `raw` value actually conforms to the expected type `V` (aside from [validating request parameters](./ch6-4-runtime-validation.md)). 

It is up to you to ensure that the data stored in KV is of the correct shape, and to handle any cases where it is not.

### Backend Helpers

For each KV template and Durable Object template, Cloesce will generate accessor methods to `get`, `put`, and `list` keys in the corresponding KV namespace.

For example, for the schema:

```cloesce
kv MyNamespace {
    settings() -> json {
        "settings"
    }

    profile(userId: string) -> json {
        "profile/{userId}"
    }
}
```

Cloesce will merge the `KVNamespace` or `DurableObject` interfaces with the following generated methods:

| Method | Description |
|--------|-------------|
| `env.settings.template()` | Returns the key template for the `settings` field, which is simply "settings" in this case. |
| `env.settings.get()` | Fetches the value at the key "settings" in `MyNamespace`. |
| `env.settings.put(value)` | Puts the given value at the key "settings" in `MyNamespace`. |
| `env.settings.list({...})` | Lists all keys in `MyNamespace` that match the prefix "settings". |
| `env.profile.template(userId)` | Returns the key template for the `profile` field, which is `"profile/{userId}"` with `{userId}` replaced by the actual value of `userId`. |
| `env.profile.get(userId)` | Fetches the value at the key `"profile/{userId}"` in `MyNamespace`, with `{userId}` replaced by the actual value of `userId`. |
| `env.profile.put(userId, value)` | Puts the given value at the key `"profile/{userId}"` in `MyNamespace`, with `{userId}` replaced by the actual value of `userId`. |
| `env.profile.list(userId, {...})` | Lists all keys in `MyNamespace` that match the prefix `"profile/{userId}"`, with `{userId}` replaced by the actual value of `userId`. |

where `env` is the Cloesce Environment.