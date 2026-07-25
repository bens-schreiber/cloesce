# Navigation Fields

When building with Cloudflare Workers, data can exist in many different places.

Modern ORMs easily answer the question:

- _"Can I represent relationships between tables in the same database?"_

But what about other data sources?

- _"Can a table have a relationship with a table in **another** database?"_
- _"...With a Durable Object?"_
- _"...KV and R2?? "_
- _"Do I even need a table to have a relationship with other data??!"_

A Cloesce Model transcends any single data source through the use of navigation fields.

## Defining Navigation Fields

A Model can declare that it has a navigation field to another Model, which can be any Model (including itself). This is done by the `one` and `many` keywords.

In either case, to define a relationship, the following _MUST_ be provided to the navigation field:

- Durable Object Shard values
- Route field values

### One-To-One

A one-to-one navigation is defined with the `one` keyword, and will only ever result in a single instance of the related Model being returned (or `undefined` if no related instance exists).

```cloesce
model Person for Db {
    primary {
        id: int
    }

    foreign Dog::id {
        dogId
    }

    one Dog::id(dogId) {
        dog
    }
}

model Dog for Db {
    primary {
        id: int
    }
}
```

In the above example, the `Person` Model has one `Dog` Model, which will be populated by matching the `Person`'s `dogId` field with the `Dog`'s `id` field.

It is not necessary to provide any discriminator (like shown in `Dog::id(dogId)`), but it may produce a more efficient query plan to do so. For example, the following is also valid:

```cloesce
model Person for Db {
    primary {
        id: int
    }

    one Dog {
        dog
    }
}

// ...
```

Here, the `Person` Model has one `Dog` Model, but how it is populated is left up to Cloesce to determine. In this case, the query planner will **scan** for the first `Dog` instance in the database, as opposed to the previous example which will **search**, utilizing the `Dog`'s primary key index to find the related instance.

### One-To-Many

A Model may hae a one-to-many relationship with any Model (including itself) by using a `many` block. This will result in an array of related instances being returned. Some Models cannot be enumerated, and will result in a singleton list being returned.

```cloesce
model Person for Db {
    primary {
        id: int
    }

    many Dog::ownerId(id) {
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

The above example defines a relationship where Person has many Dogs, which will be populated by matching the Person's `id` field with the Dog's `ownerId` field (a **search** operation).

Like with the `one` block, it is not necessary to provide any discriminator (like shown in `Dog::ownerId(id)`), but it may produce a more efficient query plan to do so. For example, the following is also valid:

```cloesce
model Person for Db {
    primary {
        id: int
    }

    many Dog {
        dogs
    }
}
// ...
```

This relationship will return all Dog instances in the database (a **scan** operation), as opposed to the previous example which will **search**, utilizing the `Dog`'s foreign key index to find all related instances.

## To Any Model? (Example)

Yes! Cloesce can represent and even hydrate navigation fields between any Model.

For example, a D1 backed Model with a one-to-one relationship to a Durable Object backed Model:

```cloesce
model PersonIndex for D1Db {
    primary {
        personId: int
        tenant: string
    }

    one PersonDo::{personId, tenant} {
        person
    }
}

model Person for PersonDo::{personId, tenant} {
    kv PersonDo::{profile, personId, tenant} {
        profile
    }

    // We can even point back to the index Model, if we want too!
    one PersonIndex::{personId, tenant} {
        index
    }

    // What if we also wanted all people in a tenant? We can do that too!
    many PersonIndex::tenant {
        allPeopleInTenant
    }
}
```

Here, the `PersonIndex` Model is backed by a D1 database, meaning it is a table `PersonIndex` in `D1Db`.

- Within every `PersonIndex` row is a logical navigation to the `Person` Model, which is backed by the Durable Object `PersonDo`.
  - We _must_ provide the shard values of `personId` and `tenant` to locate the correct `PersonDo`.

`Person` is backed by the Durable Object `PersonDo`, but is _not_ a table in that Durable Object. Instead, it is purely logical and hydrates its data from the shard fields `personId` and `tenant`. Every `Person` has:

- A `profile` which comes from the DO's own KV
- A navigation field back to the `PersonIndex` Model, which is a table in the D1 database.
- A navigation field to all `PersonIndex` rows in the same `tenant`, which is a one-to-many relationship.

### No Backing Required

It is not necessary for a Model to be backed by anything in order to have navigation fields. Models don't even need data to have a relationship! For example:

```cloesce
model Logical {
    route {
        tenant: string
    }

    many DoBacked::tenant {
        allDoBackedInTenant
    }

    one Empty {
        empty
    }

    many Empty {
        empties
    }
}

model Empty {}

model DoBacked for Do::tenant {
    primary {
        id: int
    }
}
```

In this example, the `Logical` Model is not backed by any kind of database. It exists purely from values passed from HTTP requests (`route` fields).

It has a many relationship with the `DoBacked` Model, who is backed by a Durable Object, and is a SQLite table in that Durable Object. Because no discriminator is provided, the `DoBacked` Model will be **scanned** for all instances in the same `tenant`.

Additionally, the `Logical` Model has both a one and many relationship with the `Empty` Model, which is also not backed by any kind of database. At runtime, `empty` will just be an empty object, and `empties` will be an array with a single empty object in it. This is because the `Empty` Model cannot be enumerated, and will always return a singleton list.

For more details on hydration, see the [ORM Chapter](TODO)
