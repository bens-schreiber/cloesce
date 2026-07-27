# Data Sources Overview

## What are Data Sources?

Data Sources are Cloesce’s answer to querying when there is potential for:

- overfetching
- recursive relationships
- complex business logic

Every Data Source is composed of:

- an _Include Tree_
- `get`, `list`, and `save` operations

Data Sources are used extensively in the backend, but are also exposed to the client during API generation. A client may call any of the CRUD operations on a Data Source, making them the go-to method of writing any `get`, `list` or `save` operation for a Model.

## Include Trees

To determine which fields to hydrate, Cloesce uses a construct called the _Include Tree_. An Include Tree is a recursive structure that represents the relationships between Models and their fields.

Consider the following example of a `Person` and `Dog` Model:

```cloesce
model Person for Db {
    primary {
        id: int
    }

    foreign Dog::id option {
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

    foreign Person::id {
        ownerId
    }

    one Person::id(ownerId) {
        owner
    }
}
```

`Person` has one `Dog`, and `Dog` has one `Person`.

If we were to fetch naively, we would end up in an infinite loop of fetching `Person` and `Dog` instances. To prevent this, Cloesce will generate the following Default Data Source for the `Person` and `Dog` Models:

```cloesce
source Default for Person {
    include {
        dogs
    }
}

source Default for Dog {
    include {
        owner {
            dogs
        }
    }
}
```

Each branch of the Include Tree is a relationship that will be joined when fetching a Model. Relationships can be traversed from Model to Model: if I include an `owner`, I can now include any number of relationships that belong to the `owner`.

### Default Include Tree

To prevent overfetching (and infinite loops), the Default Data Source will join:

- R2 and KV fields
- All [One-to-One Navigation Fields](./ch4-5-navigation-fields.md#one-to-one-relationship)
- The near side of all [1:M Navigation Fields](./ch4-5-navigation-fields.md#one-to-many-relationship)
- The near side of a recursive relationship

## CRUD Operations

Alongside the Include Tree, every Data Source has three operations: `get`, `list`, and `save`.

The default implementations of these operations are as so:

- `get`: fetch a single instance by primary keys, route keys and shard keys
- `list`: fetch a list of instances by primary keys, route keys and shard keys via limited seek pagination
- `save`: insert, update or upsert an instance and all children by a partial snapshot of a Model

Each default implementation will follow the Include Tree defined in a Data Source, omitting any relationships not within the tree.

For example:

```cloesce
source Default for Person {
    include {} // Empty!
}
```

The above Data Source includes _no_ relationships, so the default `get` and `list` operations will only return the `Person`'s primary keys and foreign keys, and will not join any relationships. `save` will simply no-op on children.

For more information on how the Cloesce ORM and Query Planner works, read the [ORM Chapter](./ch7-0-orm-reference.md).
