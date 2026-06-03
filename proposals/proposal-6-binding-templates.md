# Proposal: Binding Templates

- **Author(s):** Ben Schreiber
- **Status:** Draft | Review | Accepted | Rejected | **Implemented**
- **Created:** 2026-05-13
- **Last Updated:** 2026-06-03

---

# Summary

Two additions to make R2/KV keys reusable and Models composable. **Binding templates** move key construction onto the binding itself, so the same key logic can be shared instead of repeated as string literals on every Model. **Worker backed Models** are Models that hydrate from API route params instead of D1, and can hold 1:1 relationships to other Models.

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

1. Some other Model cannot compose a `User` Model because there is no way to know how to hydrate its `keyfield`.

2. If another Model were to use the same key literals for its own R2 or KV fields, it would have to repeat the same literals on its own declarations.

3. CRUD methods like `list` cannot be generated for `User` because there is no way to know how to generate the keys for listing all `User`s.

To address these problems, we will introduce two new concepts:

- **Binding templates**: A reusable template for constructing keys for R2 and KV fields, declared within the environment binding itself.
- **Worker backed Models**: A Model that is not backed by D1, but can hydrate itself from API route parameters

---

# Design

## Worker Backed Models

A Worker backed Model is a Model capable of having fields that exist on the API route itself, and are not checked against some external database call (like D1).

Worker backed Models have the following constraints:

- Cannot have one-to-many relationships to other Models
- Can only store primitive types in their `route` block
- Have no SQL representation

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

The relationship between `Person` and `Dog` is cyclical, but this is not a problem because Cloesce will utilize the default `Include Tree` and user-defined `Include Tree`s to prevent infinite recursion when resolving relationships.

Either of these Models could be referenced in a D1 backed Model, or compose a D1 backed Model, but they cannot have a one-to-many relationship to any other Model.

For example:

```cloesce
model User for Db {
    primary {
        id: int
    }

    nav Person::id(id) {
        person
    }
}

model Person {
    route {
        id: int
    }

    r2 Avatars::avatar(id) {
        avatar
    }

    kv Metadata::meta(id) {
        meta
    }

    nav User::id(id) {
        user
    }
}
```

Here, `User` is a D1 backed Model that has a one-to-one relationship to the Worker backed Model `Person`. `Person` has a `route` field `id`, which is used to navigate to `User`, as well as to retrieve the `avatar` from R2 and the `meta` from KV.

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

Here, `meta` is a template that takes an `id` parameter and returns a `json` value, and the `metas` template returns a paginated list of `json` values.

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
