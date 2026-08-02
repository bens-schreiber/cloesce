# Navigation Fields

Data can exist in many different places. Modern ORMs easily answer the question:

- _"Can I represent relationships between tables in the same database?"_

But what about more complicated system designs?

- _"Can a table have a relationship with a table in **another** database?"_
- _"...With a Durable Object?"_
- _"...KV and R2?? "_
- _"Do I even need SQL to have a relationship with data??!"_

Through **Navigation Fields** and the [Cloesce ORM](./ch7-0-orm-reference.md), Cloesce can represent relationships between any Model, regardless of where the data is stored.

## Defining Navigation Fields

A navigation field is a `one` or `many` relationship to another Model, which can be any Model (including itself).

Although the Cloesce ORM is able to operate with minimal information, the following _MUST_ be provided to any navigation field declaration:

- Durable Object Shards
- Route fields

### One-To-One

A one-to-one navigation is defined with the `one` keyword, and will only ever result in a single instance of the related Model being returned (or `undefined` if no related instance exists).

**Classic Example**

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

**No Discriminator Example**

It is not necessary to provide any discriminators to the navigation field, but it may produce a more efficient query plan to do so.

The following is also valid:

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

Here, the `Person` Model has one `Dog` Model, but no discriminator is provided. This results in a far less efficient query plan, as the ORM will **scan** for the first `Dog` instance in the database, as opposed to the previous example which will **search**, utilizing the `Dog`'s primary key index to find the related instance.

### One-To-Many

A `many` relationship suggests that any Model that matches the provided discriminator (or all instances if no discriminator is provided) will be returned in an array.

**Classic Example**

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

**No Discriminator Example**

Like with the `one` block, a discriminator is not required, but providing one may produce a more efficient query plan.

The following is also valid:

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

By default, fetching a `Person` will result in the `dogs` field containing every single dog in the database, because no discriminator was provided.

## To Any Model?

Yes! Cloesce can represent and even hydrate navigation fields between any Model.

**D1 to Durable Object Example**

```cloesce
model PersonIndex for D1Db {
    primary {
        personId: int
        tenant: string
    }

    one Person::{personId, tenant} {
        person
    }
}

model Person for PersonDo::{personId, tenant} {
    kv PersonDo::{profile, personId, tenant} {
        profile
    }

    // We can even point back to the index Model!
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

**Worker Backed Example**

It is not necessary for a Model to be backed by anything in order to have navigation fields.

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

For more details on hydration, see the [ORM Chapter](./ch7-0-orm-reference.md)
