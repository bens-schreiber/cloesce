# Data Sources Overview

If you query an instantiated [API](./ch6-1-rest-apis.md) endpoint of a [Model](./ch4-0-models.md), you may notice that Cloesce will leave undefined values or empty arrays in deeply nested compositions with other Models. 

This is intentional, and an effect of Data Sources.

## What are Data Sources?

Data Sources are Cloesce’s response to the overfetching and recursive relationship challenges when modeling relational databases with object-oriented paradigms.

Every Data Source is composed of an _Include Tree_, along with `get`, `list`, and `save` operations. Include Trees are necessary to determine which fields to hydrate when fetching data for a given Model.

For example, in the Model definition below, how should Cloesce know how deep to go when fetching a Person and their associated Dog?

```cloesce
model Dog for Db {
    primary {
        id: int
    }

    foreign Person::id {
        ownerId
    }

    nav Person::id(ownerId) {
        owner
    }
}

model Person for Db {
    primary {
        id: int
    }

    nav Dog::ownerId {
        dogs
    }
}

// => { id: 1, dogs: [ { id: 1, owner: { id: 1, dogs: [ ... ] } } ] } ad infinitum
```

If we were to follow this structure naively, fetching a `Person` would lead to fetching their `Dog`, which would lead to fetching the same `Person` again, and so on, resulting in an infinite loop of data retrieval.

## Default Data Source

Every Model will come with a Default Data Source (called `Default`) that Cloesce will use whenever an operation does not explicitly specify a Data Source to use.

### Default Include Tree

To prevent overfetching (and infinite loops), the Default Data Source will only join:

- Any R2 and KV fields
- All [1:1 Navigation Fields](./ch4-5-navigation-fields.md#one-to-one-relationship)
- The near side of all [1:M Navigation Fields](./ch4-5-navigation-fields.md#one-to-many-relationship)

## Include Trees

To determine which fields to hydrate, Cloesce uses a construct called the _Include Tree_. An Include Tree is a recursive structure that represents the relationships between Models and their fields, and is used by Cloesce to determine how to fetch data for a given Model.

For example, in the `Person` and `Dog` Models above, the Default Data Source's Include Tree would join only the field `dogs` on `Person`, but on the `Dog` Model, it would join `owner`, and then finally `dogs`.

```cloesce
// Include Tree for Person
include {
    dogs
}

// Include Tree for Dog
include {
    owner {
        dogs
    }
}
```
