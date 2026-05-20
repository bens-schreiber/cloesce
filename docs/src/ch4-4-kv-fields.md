# KV Fields

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value store. Cloesce provides first class support for KV, allowing you to define Models with KV-hydrated fields that seamlessly integrate with [D1](./ch4-1-d1-backed-model.md) backed Models and [R2](./ch4-5-r2-fields.md) fields.

## Features of KV

> [!NOTE]
> KV Fields in the schema do not yet support cache control directives and expiration times. This feature is planned for a future release.

Cloudflare KV has a simple toolset and API for storing and retrieving key-value pairs. Specifically, KV supports:

- Eventually consistent writes
- Sub 5ms read latency on cached reads
- List queries with key based prefix matching and pagination
- Deletion of individual keys or entire namespaces
- Custom metadata on each key-value pair
- 25MB value size limit

Cloesce respects the design constraints of KV storage. For Models with only KV fields, the following features are **not** supported:

- Relationships
- Navigation fields
- Migrations

## Defining an Environment Binding

To use KV fields in your Models, you first need to define an environment binding for the KV namespace in your Cloesce schema. This is done using the `env` block, where you specify the KV namespaces your application will use.

```cloesce
env {
    kv {
        my_namespace
    }
}
```

In the above example, we have defined a KV environment binding called `my_namespace`. This binding will be used to reference the KV namespace in our Model definitions. Cloesce will generate all necessary Wrangler configurations and typed backend code to seamlessly integrate this KV namespace into your application.

## Defining a KV Field

A field in a Model can exist in Cloudflare KV by using the `kv` block, which specifies the binding and key for the field:

```cloesce
model Settings {
    kv (my_namespace, "settings") {
        theme: string
    }
}
```

The above snippet defines a Model `Settings` with a KV field `theme` that is stored in the namespace `my_namespace` under the static key "settings". `theme` is typed as a string, and Cloesce will automatically handle the serialization and deserialization of this field when reading from and writing to KV.

### Quirks of a non-D1 backed Model

In a [D1 backed Model](./ch4-1-d1-backed-model.md), when querying an instantiated API method, if the Model at some primary key does not exist, Cloesce will return a `404` error.

However, in a Model with only KV fields (or R2 fields), there is no underlying "backing". That is to say, if a value does not exist for a particular key, there is no way for Cloesce to know whether that key is supposed to exist with a `null` value, or if it simply does not exist at all.

For this reason, when querying an instantiated API method on a non-D1 backed Model, you may find `null` values for fields that do not exist in KV, rather than a `404` error. It is up to you as the developer to handle this case appropriately in your application code.

## Key Fields and Interpolation

It is likely that a static key will not be sufficient for all use cases. For example, you may want to have multiple settings objects for different users, each stored under a dynamic key based on the user ID.

To support this, Cloesce allows you to define a `keyfield`, a field that exists only in the URL route (e.g. `/settings/:userId`) and can be used in combination with string interpolation to create dynamic keys for your KV fields.

```cloesce
model Settings {
    keyfield {
        userId: string
    }

    kv (my_namespace, "settings/{userId}") {
        theme: string
    }
}
```

In this example, we have defined a `keyfield` called `userId`, which is a string that will be provided in the URL route. The KV field `theme` is stored under the key "settings/{userId}", where `{userId}` is replaced with the actual value of the `userId` keyfield when accessing KV.

### D1 Column Interpolation

Key fields are not the only way to create dynamic keys for your KV fields. You can also reference D1 columns in your KV key definitions, allowing you to create dynamic keys based on the values of your D1 backed Models.

```cloesce
model User {
    primary {
        id: int
    }

    column {
        name: string
    }

    kv (my_namespace, "user_settings/{id}") {
        theme: string
    }
}
```

The Cloesce ORM will know to first hydrate the table `User` to get the value of `id`, and then use that value to construct the key for the KV field `theme`, finally fetching the value from KV.

This allows you to easily associate KV data with your D1 backed Models without having to manually handle the logic of fetching from D1 and then using that data to fetch from KV (one Model to rule them all!).

## Paginated List Queries

Cloesce also supports paginated list queries for Models with KV fields. This allows you to retrieve multiple key-value pairs from KV that share a common prefix.

```cloesce
model Settings {
    kv (my_namespace, "settings/") paginated {
        allSettings: json
    }
}
```

Here, the `paginated` modifier on the `kv` block indicates that we want to retrieve all key-value pairs in the `my_namespace` namespace that have keys starting with "settings/". The results will be returned in a paginated format, allowing you to handle large datasets efficiently.

## Generated Types

For both the backend and the client, Cloesce utilizes a `KValue` wrapper class to represent the data retrieved from KV.

```ts
export class KValue<V> {
  key: string;
  raw: unknown | null;
  metadata: unknown | null;

  get value(): V | null {
    return this.raw as V | null;
  }
}
```

This class encapsulates the key, raw value, and metadata of a KV entry. The `value` getter provides a typed view of the raw value, though, Cloesce will not validate that the raw value actually conforms to the expected type `V`.

Metadata is left as `unknown` because it can be any arbitrary JSON object that you choose to associate with the key-value pair in KV. Cloesce does not impose any structure on this metadata, allowing you to use it for any purpose you see fit (e.g. storing timestamps, user information, etc.).
