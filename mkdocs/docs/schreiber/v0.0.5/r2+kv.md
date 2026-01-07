# Thoughts on R2 and KV

In the Cloesce abstract, we describe a tool that  "orchestrates the database, backend, client and infrastructure". In this version, we will change the word "database" to be a more generic term "data".

For our purposes, data can be defined as anything that can be stored and retrieved in some persistent way. This includes relational databases (D1), object storage (R2) and key-value stores (KV).

With that in mind, Cloesce should be capable of orchestrating not just D1, but also R2 buckets and KV namespaces. The current `Model` paradigm must be extended to support these new data stores. A `Model` should be able to define not just tables and columns, but also R2 buckets and KV namespaces, in any structure the user desires (e.g, a `Model` could have D1 data, R2 data and KV data all in one, or just R2 data, D1 data, any combination).

## KV 

Cloudflare KV is a simple storage platform capable of associating a key (which must be a string) with a value (which can be text, json, bytes, etc). Additionally, JSON metadata can be stored with each key. KV is schema-less, meaning you can throw any value into any key and face no problems. Cloesce will not try to enforce a schema on KV data, but instead will provide a simple interface to store and retrieve data.

Looking at the properties of KV, several features stand out:
- Data can be listed by key prefix, ie list all keys that start with "user"
- Data can have expiration times set, after which the data is automatically deleted
- Data can have metadata associated with it
- If a value does not exist for a given key, null is returned (no error is thrown)

Let's first consider a Model that defines only KV data:

```ts
/**
 * Return type for any KV value.
 * 
 * V can be any Cloesce serializeable type. There is no guarantee that the value
 * is actually of type V.
 */
class KValue<V> {
    key: string;
    raw: unknown;
    value: V | null; // No guarantees it is a V.
    metadata: unknown;
}

@Model
class User {
    @KeyParam
    id: string;

    @KV("user:{id}", "namespace")
    userData: KValue<unknown>; // `unknown` can represent any JSON Value.

    @KV("favoriteNumber:user:{id}", "namespace")
    favoriteNumber: KValue<number>; // Cloesce will try to cast to number. No promises.

    @KV("user", "namespace")
    allUsers: KValue<unknown>[]; // List all keys with prefix "user" and then hydrates them.

    @DataSource
    static readonly default: IncludeTree<User> = {
        userData: {},
        favoriteNumber: {},
        allUsers: {}
    }
}
```

In this example, `User` consists of four fields:
- `id`: Decorated with `@KeyParam`, this field is used to fill in the `{id}` placeholder in the `@KV` decorators.
- `userData`: This field represents a KV entry with a key formatted as "user:{id}" in the "namespace" KV namespace. The value can be any JSON-serializable type, denoted by the `unknown` type.
- `favoriteNumber`: This field represents a KV entry with a key formatted as "favoriteNumber:user:{id}" in the "namespace" KV namespace. The value is expected to be a number, but Cloesce will attempt to cast it to a number without guarantees (NaN is possible).
- `allUsers`: This field represents a list of all KV entries with the prefix "user" in the "namespace" KV namespace. Cloesce will list all keys with this prefix and hydrate them into an array of `KValue<unknown>`.

Many `KeyParam` decorators can be defined, all of type string. The Cloesce runtime will substitute them into the `KV` formats as needed.

Noteably, Cloesce does not care if all fields are non-null. If a key does not exist in KV, the corresponding field will simply be `null`. This allows for a flexible data model that can evolve over time without breaking existing data, contrary to D1 models which require strict schema adherence.

### KV with D1

Cloesce models can also combine KV data with D1 data. For example, consider a `User` model that stores basic user information in D1, but stores user preferences in KV:

```ts
@Model
class User {
    @PrimaryKey
    id: string;

    name: string;

    @KV("userPreferences:{name}:{id}", "namespace")
    preferences: KValue<unknown>; // User preferences stored in KV.

    @DataSource
    static readonly default: IncludeTree<User> = {
        preferences: {}
    }
}
```

D1 columns can be used within a `KV` string, allowing for dynamic key generation based on D1 data. In this example, the `preferences` field uses both the `name` and `id` fields from D1 to construct the KV key.

Importantly, a D1 row must actually exist for the KV data to be accessed. If there is no D1 row for a given primary key, a 404 will be returned from the API. `preferences` on the other hand can be null even if the D1 row exists.

### CRUD

CRUD operations should be supported when integrating with KV data. `GET` will take in the necessary key parameters, and `SAVE` will take validated JSON data to store in KV.

However, the `LIST` operation doesn't make as much sense when dealing with purely KV fields. Thus, it won't be supported unless a D1 model is also present to provide context for listing.

## R2

Cloudflare R2 is an object storage platform that allows for storing and retrieving large binary objects. R2 is schema-less, similar to KV, meaning you can store any type of data in any bucket. R2 also supports a similiar feature set to KV, such as prefix listing and metadata.

To represent R2 data in Cloesce models, we can use the Cloudflare `R2Object` type, which contains the objects key, size, etag, lastModified, metadata and other properties. Unlike KV, no generic type parameters are needed since R2 objects are binary blobs. Only a `ReadableStream` will be returned when retrieving the object data such that large files can be streamed efficiently without buffering the entire file in memory.

Let's consider a Model that defines only R2 data:

```ts
@Model
class UserProfilePicture {
    @KeyParam
    userId: string;

    @R2("profile-pictures/{userId}.png", "user-bucket")
    profilePicture: R2Object | null; // R2 object or null if not found.

    @R2("profile-pictures/", "user-bucket")
    allPictures: R2Object[]; // List all objects in the bucket.

    @DataSource
    static readonly default: IncludeTree<UserProfilePicture> = {
        profilePicture: {},
        allPictures: {}
    }
}
```

In this example, `UserProfilePicture` consists of three fields:
- `userId`: Decorated with `@KeyParam`, this field is used to fill in the `{userId}` placeholder in the `@KV` decorator.
- `profilePicture`: This field represents an R2 object with a key formatted as "profile-pictures/{userId}.png" in the "user-bucket" R2 bucket. The value is of type `R2Object` or `null` if the object is not found.
- `allPictures`: This field represents a list of all R2 objects in the "user-bucket" R2 bucket with the prefix "profile-pictures/". Cloesce will list all objects with this prefix and hydrate them into an array of `R2Object`.

### R2 with D1

Cloesce models can also combine R2 data with D1 data. For example, consider a `User` model that stores basic user information in D1, but stores user profile pictures in R2:

```ts
@Model("my-database")
class User {
    @PrimaryKey
    id: string;

    name: string;

    @R2("profile-pictures/{id}.png", "user-bucket")
    profilePicture: R2Object | null; // User profile picture stored in R2.

    @DataSource
    static readonly default: IncludeTree<User> = {
        profilePicture: {}
    }
}
```

Just like in KV, D1 columns can be used within a `R2` format string, allowing for dynamic key generation based on D1 data. In this example, the `profilePicture` field uses the `id` field from D1 to construct the R2 object key.

D1 is still the source of truth for the existence of a user. If there is no D1 row for a given primary key, a 404 will be returned from the API. `profilePicture` on the other hand can be null even if the D1 row exists.

### CRUD

- `GET`: Retrieve R2 metadata and all other associated fields/columns. Object data is not streamed since mixing JSON and binary data is not feasible.
- `SAVE`: Save will ignore R2 fields since uploading binary data is not feasible in a JSON API. It is up to the user to upload R2 objects separately.
- `LIST`: Similiar to KV, listing R2 objects only makes sense when a D1 model is present to provide context. Thus, it won't be supported unless a D1 model is also present.

### Signed URLs

R2 supports generating signed URLs for secure, temporary access to objects (upload and download). In the future, this should be supported for Cloesce, but is out of scope for v0.0.5.


## Sending Key Params over HTTP

The Cloesce router expects method invocations to hit endpoints in the form of `/{Model}/{Id}/{Method}` or `/{Model}/{Method}` static methods. With the addition of the `@KeyParam` decorator, multiple key parameters may be needed to uniquely identify a model instance. 

To accommodate this, the router will be updated to accept multiple key parameters in the URL path. The order of the key parameters will be the model primary key first (if applicable), followed by any additional `@KeyParam` decorated fields in the order they are defined in the model.

For example, consider the following model:

```ts
@Model
class User {
    @KeyParam
    userId: string;

    @KeyParam
    profileId: string;

    @KeyParam
    configId: string;

    // ...
}
```

To access a method on this model, the endpoint would be structured as follows:

`UserProfile / {userId} / {profileId} / {configId} / {Method}`

Later this pattern will be expanded on when composite primary keys are supported.
