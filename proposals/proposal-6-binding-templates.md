# Summary

---

# Motivation

Each Model must declare the location at which its data is stored in the environment. For KV and R2 fields, this is done with a string literal to specify the key or prefix for the field. Additionally, if a Model does not have any D1 backing, a `keyfield` can be declared to have route-level identifiers. For example:

```cloesce
env {
    r2 {
        avatars
    }

    kv {
        metadata
    }
}

model User {
    keyfield {
        id: int
    }

    r2 (avatars, "key/{id}") {
        avatar
    }

    kv (metadata, "metadata/{id}") {
        meta: json
    }

    kv (metadata, "metadata/") paginated {
        metas: json
    }
}
```

This has several problems:

1. Some other Model cannot compose a `User` Model because there is no way to know how to hydrate it's `keyfield`

2. If another Model was to use the same key literals for its own R2 or KV fields, it must repeat the same literals on its own declarations.

3. CRUD methods like `list` cannot be generated for `User` because there is no way to know how to generate the keys for listing all `User`s

To address these problems, we will introduce two new concepts:

- **Binding templates**: A reusable template for constructing keys for R2 and KV fields, declared within the environment binding itself.
- **Worker backed Models**: A Model that is not backed by D1 and cannot have relationships to D1 backed Models, but can have relationships to other Worker backed Models.

```cloesce
model Foo {
    // Declares that Foo::personId is foreign to Person::id
    foreign Person::id {
        personId

        // Declares that the Person object can be accessed by navigating from Foo to Person via the id field
        nav {
            person
        }
    }

    // Declares that Foo::bar has many Bar's joined on Bar::barId's foreign key relationship to Foo
    // Translates to an array of Bar objects
    nav Bar::barId {
        bars
    }
}

// new proposed syntax
model Foo {
    // same as before, but cannot have a `nav` block inside anymore
    foreign Person::id {
        personId
    }

    // 1:1 relationship to Person, navigable by `personId`
    nav Person::id(personId) {
        person
    }

    // 1:many relationship to Bar, navigable by `barId`
    nav Bar::barId {
        bars
    }
}

// new proposed syntax for navigation properties with route based models
model User {
    route {
        id: int
    }

    nav Dog::ownerId(id) {
        dog
    }
}

model Dog {
    route {
        ownerId: int
    }

    nav User::id(ownerId) {
        owner
    }
}

// composite keys example
model User {
    route {
        id: int
        tenant: int
    }

    nav Dog::ownerId(id) {
        dog
    }
}

model Dog {
    route {
        ownerId: int
        tenant: int
    }

    nav (User::id(ownerId), User::tenant(tenant)) {
        owner
    }
}
```

---

# Design

## Worker Backed Models

A Worker backed Model is a Model capable of having fields that exist on the API route itself, and are not checked against some external database call (like D1).

Worker backed Models have the following constraints:

- Cannot have relationships to D1 backed Models (yet!)
- Cannot have one-to-many relationships
- Can only store primitive types in it's `route` block
- Have no SQL representation to them

For example:

```cloesce
model Person {
    route {
        id: int
    }

    nav Dog::ownerId(id) {
        dog
    }
}

model Dog {
    route {
        ownerId: int
    }

    nav Person::id(ownerId) {
        owner
    }
}
```

In this example, `Person` is a Worker backed Model. It has a `route` field `id` which exists only in the API invocation: `GET /Person/:id`.

`Person` has a one-to-one relationship to `Dog`, navigable by the `id` field on `Person` and the `ownerId` field on `Dog`. Unlike in a D1 Model where some `JOIN` must occur, here no call needs to be made to resolve the relationship. The runtime can simply take the value of `id` from the `Person` route, and use it to retrieve the corresponding `Dog`.

The relationship between `Person` and `Dog` is cyclical, but this is not a problem because Cloesce will utilize the Default `Include Tree` and user defined `Include Tree`s to prevent infinite recursion when resolving relationships.

## Modifying D1 Backed Models Syntax

The syntax for declaring relationships in a D1 backed Model will change slightly to be more consistent with the syntax for relationships in a Worker backed Model. For example:

```cloesce
model Person for Db {
    primary {
        id: int
    }

    foreign Dog::ownerId {
        dogId
    }

    nav Dog::id(dogId) {
        dog
    }
}

model Dog for Db {
    primary {
        id: int
    }
}
```

Here, `Person` has a one-to-one relationship to `Dog` (dogId joins to Dog::id). The `foreign` block declares that `dogId` is a foreign key to `Dog::id`, and the `nav` block declares that the relationship is navigable by `dogId`.

A change from the previous syntax is that the `nav` block is no longer nested inside the `foreign` block. Semantic analysis will still ensure that the `nav` block is correctly associated with the `foreign` block that declares the relevant foreign key, but this new syntax is more consistent with the syntax for relationships in Worker backed Models.

One-to-many relationships will use the same syntax as before (dropping the parenthesis unless the key is composite):

```cloesce
model Person for Db {
    primary {
        id: int
    }

    nav Dog::ownerId {
        dogs
    }
}

model Dog for Db {
    primary {
        id: int
    }

    foreign Person::id {
        ownerId
    }
}
```

## R2 Binding Template

An R2 declaration will consist of the binding name, followed by any number of templates. Each template consists of a name and a parameter list.

R2 templates implicitly return an `R2Object` type, and thus specifying a type is not necessary.

```cloesce
r2 UserAvatars {
    avatar(id: int) {
        "key/{id}"
    }
}
```

Here, `UserAvatars` is the binding name, and `avatar` is a template that takes an `id` parameter of type `int`. The body of the template is a string literal that specifies how to construct the key for retrieving the avatar from R2. The `{id}` syntax indicates that the value of the `id` parameter should be interpolated into the string.

### Paginated Templates

To represent a paginated list of objects, we can use the `paginated` infix keyword:

```cloesce
r2 UserAvatars {
    avatar(id: int) {
        "key/{id}"
    }

    avatarList() paginated {
        "key/"
    }
}
```

## KV Binding Template

KV declarations follow a similar pattern to R2, but with the addition of a return type for each template:

```cloesce
kv UserMetadata {
    meta(id: int) -> json {
        "metadata/{id}"
    }

    metas() -> paginated<json> {
        "metadata/"
    }
}
```

Here, `meta` is a template that takes an `id` parameter and returns a `json` value. The `metas` template returns a paginated list of `json` values.

## Referencing Binding Templates

Once a binding template is declared, it can be referenced in any Model or API. For example:

```cloesce
model User {
    route {
        userId: int
    }

    kv UserMetadata::meta(userId) {
        meta
    }

    r2 UserAvatars::avatar(userId) {
        avatar
    }
}
```

In this example, the `User` model references the `meta` template from the `UserMetadata` KV binding and the `avatar` template from the `UserAvatars` R2 binding. The parameters for each template must match the types of the parameters declared in the binding template.

Types are inferred based on the return type of the template. For example, `meta` will be inferred to have the type `json` because that is the return type declared in the `UserMetadata` binding.

### Inheriting Validators

If a binding template has validators declared on any of its parameters, any Model that references the template will inherit those validators on the corresponding fields. For example:

```cloesce
kv UserMetadata {
    meta([gt 0] id: int) -> json {
        "metadata/{id}"
    }
}

model User {
    route {
        userId: int
    }

    kv UserMetadata::meta(userId) {
        userMeta
    }
}
```

Because the field `userMeta` references the `meta` template, and passes `userId` as the `id` parameter, `userId` will inherit the `[gt 0]` validator from the `meta` template.

Additionally, KV templates can have validators declared on the return type, which will also be inherited by any referencing fields. For example:

```cloesce
kv UserMetadata {
    [len 10]
    meta(id: int) -> string  {
        "metadata/{id}"
    }
}

model User {
    route {
        userId: int
    }

    kv UserMetadata::meta(userId) {
        userMeta
    }
}
```

In this case, `userMeta` will inherit the `[len 10]` validator from the `meta` template.

## Generating Code for Binding Templates

Previously, all R2 and KV fields declared in a Model would have a `Key` namespace generated for them, which contained methods for interpolating keys defined in the schema.

With the new binding templates system, the `Key` namespace will leave the Model and instead be generated on the binding itself. For example, given the following binding:

```cloesce
r2 UserAvatars {
    avatar(id: int) {
        "key/{id}"
    }
}
```

The following code will be generated:

```ts
export class UserAvatars {
    static avatar(id: number) => `key/${id}`;
}
```

This way, any Model or API can generate keys for the `avatar` template by referencing `UserAvatars::avatar(id)`.

Note that this is separate from the actual binding code that is inside the `Env` interface. The `UserAvatars` class is a generated helper for constructing keys, while the `UserAvatars` binding in the `Env` interface is used for retrieving data from R2.

## Field Accessors

Parameters to a binding template can access fields of objects that are passed in as arguments. For example:

```cloesce
model User {
    route {
        userId: int
    }
}

poo Friend {
    user: User
}

kv UserMetadata {
    meta(user: User, friend: Friend) -> json {
        "metadata/{user::userId}/{friend::user::userId}"
    }
}
```

Given that the object passed in is a valid object in the schema, the `userId` field of the `user` parameter and the `userId` field of the `friend` parameter will be accessible within the template body using the `{param::field}` syntax.

The accepted types of a parameter in a binding template can be:

- Primitive types (e.g. `int`, `string`, etc.)
- Models (e.g. `User`)
- Plain Old Objects (POOs) (e.g. `Friend`)
- Partial object types (where a missing field is serialized to `"null"`)

Types that cannot be represented in the schema would be:

- `KValue<T>`
- `R2Object`
- `stream`
- `array<T>`
- `paginated<T>`

## Chaining KV/R2 Retrievals

It may be desirable to retrieve a value from KV, and then use that value to construct the key for another retrieval from KV. For example, Durable Objects often store data within their own KV namespace, but some value in that metadata may be needed to construct the key for retrieving the actual data.

A schema that requires "chaining" might look like this:

```cloesce
kv UserMetadata {
    meta(id: int) -> Metadata {
        "metadata/{id}"
    }

    settings(meta: Metadata) -> Settings {
        "data/{meta::metaId}"
    }
}

r2 UserAvatars {
    avatar(settings: Settings) {
        "avatars/{settings::avatarId}"
    }
}

model User for Db {
    primary {
        userId: int
    }

    kv UserMetadata::meta(userId) {
        meta
    }

    kv UserMetadata::settings(meta) {
        settings
    }

    r2 UserAvatars::avatar(settings) {
        avatar
    }
}
```

In order to retrieve `avatar` for a `User`, we need first retrieve `settings`, which requires retrieving `meta`, which requires retrieving `userId` from the `User` table in D1.

Currently, Cloesce is only capable of retrieving `User` from D1, followed by `meta` from KV, but there is no way to express further dependencies. Further, `meta` and `settings` are `KValue<T>`'s which means they are not accepted as parameters to other templates.

In order to support this kind of retrieval chaining, we will need to:

1. Allow templates to implicitly destructure `KValue<T>` into `T` when passed as parameters to other templates.
2. Modify the `map` ORM method to return a **plan**, a list of retrieval operations that the runtime can execute in order
3. Modify the runtime to be able to execute a plan, including handling errors and blocking dependent retrievals if a retrieval fails.

### `map` Plan Generation

The current sequence of steps to fully hydrate a Model is:

1. Execute the `select` query to retrieve the SQL Model from D1
2. Input SQL rows into the `map` function, which generates a JSON object that matches the shape of the Model, without any KV or R2 fields resolved.
3. Call the `hydrate` method (which does not exist in the ORM, runtime specific) to retrieve KV and R2 fields, as well as any fields that require hydration (e.g. `date`, `blob`, etc.)
4. Return the fully hydrated Model instance

Rather than having the runtime `hydrate` function be responsible for figuring out which fields to retrieve and in which order (which has to be implemented in every language runtime separately), we will modify `map` to return not only the JSON object, but also a plan for how to retrieve the remaining fields from KV and R2.

The returned plan can be interpreted and executed by the runtime, leaving only the actual fetching of data to be implemented in the runtime.

TODO
