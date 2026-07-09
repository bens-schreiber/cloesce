# Proposal: Cloesce ORM v2

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 06-20-2026
- **Last Updated:** 06-20-2026

---

## Summary

---

## Motivation

Imagine the case where we are making a Reddit clone, where every `User`, `SubReddit` and `Post` are different durable objects, and `Comment` is a SQLite table on the `Post` DO:

A `User` has

- Many `SubReddit`s they are subscribed to
- Many `Post`s they have made
- Many `Comment`s they have made
- Metadata (e.g. username, email, etc.)

A `SubReddit` has

- Many `User`s subscribed to it
- Many `Post`s made in it
- Metadata (e.g. name, description, etc.)

A `Post` has

- One `User` that made it
- One `SubReddit` it was made in
- Many `Comment`s made on it
- Metadata (e.g. title, upvotes, etc.)

A `Comment` has

- One `User` that made it
- One `Post` it was made on
- Metadata (e.g. content, upvotes, etc.)

Despite this being an intuitive data model, Cloesce in its current form cannot express the relationships between these Models because they are all separate durable objects.

This proposal aims to break the barriers Cloesce has set up:

- Every Model should be able to have a `one` or `many` relationship with _any_ other Model.
- Every Model can have `route` fields
- Every Model should be capable of having any CRUD operation

---

## Detailed Design

### Navigation Kinds

The current Cloesce schema represents relationships between Models with a `nav` block:

```cloesce
// D1 backed Model
model User for Db {
    // ...

    // 1:1
    nav Friend::id(friendId) {
        // `id` here is the primary key of the `Friend` model,
        // and `friendId` is a field on the `User` model that references it.
        friend
    }

    // 1:N
    nav Post::userId {
        // `userId` is a field on the `Post` model that is an FK to the `User` model.
        posts
    }
}

// Worker backed Model
model User {
    // ..

    // 1:1
    nav Friend::id(friendId) {
        // `id` here is some `route` field on the `Friend` model
        friend
    }

    // 1:N
    // Cannot happen! No way to index a Worker backed Model, so we can't have 1:N.
}

// Durable Object backed Model (sqlite)
model User for UserDo(userId) {
    // ..

    // ... same as D1 backed Model
}
```

Despite having similar syntax, none of these `User` models could express a relationship between one another, because the ORM is incapable of resolving complex hydration queries spannning several steps.

But lets assume that the ORM could resolve these complex hydration queries, what would the syntax look like?

To disambiguate relationships, we will remove the `nav` block and replace it with a `one` or `many` block that explicitly states the cardinality of the relationship.

### Rules of Navigation

1. Every Model can have a `one` or `many` relationship with every other Model
2. Route fields and Durable Object shard fields _must always_ be supplied in the initializer of a navigation block
3. Any other field can be used to resolve a relationship, but it must be supplied in the initializer of a navigation block

### New Syntax

The `nav` block will be replaced with a `one` or `many` block, which explicitly states the cardinality of the relationship.

A new "spider initializer" `::{}` will be introduced, which allows explicit specification of the fields that are used to resolve the relationship. This form can be used if one or more fields are required, where the old direct form can be used if one field is required.

#### Empty Form

```cloesce
model Foo {
    one ModelB {
        modelB
    }


    many ModelB {
        modelBs
    }
}
```

The most simple form of a navigation field is the empty form, where no fields are passed to resolve the relationship.

If `ModelB` has no `route` or `shard` fields, then this is a valid form. 

If `ModelB` is SQL based, then Cloesce will simply query for every single `ModelB` in the database, and return the first result for a `one` relationship, or an array of results for a `many` relationship.

#### Singular Form

```cloesce
model Foo {
    route {
        userId: int
        modelBId: int
    }


    one ModelB::id(modelBId) {
        modelB
    }

    many ModelB::userId(userId) {
        modelBs
    }
}
```

In the singular form, a single field is passed to resolve the relationship. This could be a `route`, `shard`, `primary`, or any other field on the Model. The ORM will use this field to query for the related Model.

- Unlike the old syntax, the singular form requires a value to be passed to the initializer.

- If only a Durable Object shard field is supplied, then the ORM will query for every single Model on that DO, and return the first result for a `one` relationship, or an array of results for a `many` relationship.

- If only one `route` field is supplied, then the ORM will query for every single Model on that route, and return the first result for a `one` relationship, or an array of results for a `many` relationship.

#### Plural Form (Spider Initializer)

```cloesce
model Foo {
    route {
        userId: int
        modelBId: int
        modelBDoId: string
    }

    one ModelB::{id(modelBId), doId(modelBDoId)} {
        modelB
    }

    many ModelB::{userId(userId), doId(modelBDoId)} {
        modelBs
    }
}
```

Sometimes, a relationship may require multiple fields to resolve. In this case, the "spider initializer" form can be used, which allows multiple fields to be passed to the initializer.

- The plural form can cover all scenarios of the singular form.

- The plural form cannot represent the empty form (it expects at least one field to be passed to the initializer).

#### Accessing Durable Object KV

Any Model may now use a Durable Objects KV storage, but a shard field must be supplied to the initializer should it be required to resolve the storage:

```cloesce
durable DurableObjectKV {
    shard {
        userId: int
    }

    value(userId: int, otherUserId: int) {
        "value/{userId}/{otherUserId}"
    }
}

model Foo {
    route {
        userId: int
        otherUserId: int
    }

    kv DurableObjectKV::{value(userId, otherUserId), doId(userId)} {
        value
    }
}
```

A Durable Object without any shard fields, or a normal KV store can use the singular form to access the storage:

```cloesce
durable DurableObjectKV {
    value(userId: int, otherUserId: int) {
        "value/{userId}/{otherUserId}"
    }
}

model Foo {
    route {
        userId: int
        otherUserId: int
    }

    kv DurableObjectKV::value(userId, otherUserId) {
        value
    }
}
```

### Example: Reddit Clone

Using the syntax described above, we can now express the relationships between our `User`, `SubReddit`, `Post` and `Comment` models in a Reddit clone.

To demonstrate a small example, assume that:

- `UserDo`, `SubRedditDo` and `PostDo` are all Durable Objects
- `User`, `SubReddit`, `Post` and `Comment` are Models backed by each respective Durable Objects

```cloesce
model User for UserDo(userId) {
    many UserFollowedSubReddit::userId(userId) {
        followed
    }

    many UserComment::userId(userId) {
        comments
    }

    many UserPost::userId(userId) {
        posts
    }
}

// EX:
model UserFollowedSubReddit for UserDo(userId) {
    primary {
        subRedditId: int
    }

    // This nav would populate the associated `SubReddit` Model for a `User`
    one SubReddit::id(subRedditId) {
        subReddit
    }
}

// ...same pattern for `UserComment` and `UserPost`:
// have a `primary` field for the id of the associated DO, then a `one` nav
// to the associated Model on that DO.
```

---

## Implementation

### Query Planner

When breaking all of the barriers between Models down, a weight is taken off of the shoulders of semantic analysis and placed on that of the ORM.

Instead of the ORM consisting of several functions: `select`, `map`, and `upsert`, the ORM will now consist of a single function: `plan` (`validate_types` will still be a separate function available to the runtime).

This new "query planner" will be responsible for consuming a query and producing a plan for how to execute it on the runtime, such that the runtime requires no knowledge of the relationships between Models or the underlying schema: simply execute the plan.

Plans consist of a series of transactions, and each transaction consists of a series of steps. Within a transaction, all steps can be executed in parallel, but transactions must be executed sequentially.

Each transaction can depend on the results of the previous transaction. Each step can store a result in the output of its own transaction, which can be used as input to the next transaction.

If a step fails, the entire transaction does not fail: any steps that depend on the failed steps output will be skipped. Errors will be propogated via a sink; any number of errors can occur when a query plan is executed, no single error will halt the execution of the plan.

To support this approach, two changes will be made:

1. `map` will be removed from the ORM completely.
2. Models will _no longer_ be `JOIN`ed together in SQL, and instead resolved in separate queries, regardless of whether they are in the same database or not.

### GET, LIST Plans

The Query Planner will accept a command `GET Model` or `LIST Model` with Include Tree provided, and will produce a plan for how to execute the query on the runtime.

These plans will accept arguments for the primary keys, route fields, shard fields, and filters like `limit` for `LIST` queries.

For example, to hydrate the Model:

```cloesce
model User for UserDo(userId) {
    primary {
        id: int
    }

    foreign Dog::id {
        dogId
    }

    one Dog::id(dogId) {
        dog
    }

    many Cat::userId(id) {
        cats
    }
}

// ...cat model, dog model, both on a D1 Database `AnimalDb`
```

NOTE: The JSON structure of the below plans are not final and are subject to change.

#### `GET User`

A plan for `GET User` with `IncludeTree` `{ dog, cats }` would look like this in JSON:

```json
[
    [
        {
            "db": {
                "name": "UserDo",
                "args": {
                    "from_params": ["userId"]
                }
            },
            "sql": {
                "query": "SELECT * FROM User WHERE id = ?1 ORDER BY id ASC",
                "args": {
                    "from_params": ["id"]
                },
                "map": {
                    "cardinality": "one",
                }
            },
            "result": ""
        },
    ],
    [
        {
            "db": {
                "name": "AnimalDb",
                "args": []
            },
            "sql": {
                "query": "SELECT * FROM Dog WHERE id = ?1 ORDER BY id ASC",
                "args": {
                    "from_result": ["dogId"]
                },
                "map": {
                    "cardinality": "one",
                }
            },
            "result": "dog"
        },
        {
            "db": {
                "name": "AnimalDb",
                "args": []
            },
            "sql": {
                "query": "SELECT * FROM Cat WHERE userId = ?1 ORDER BY id ASC",
                "args": {
                    "from_result": ["id"]
                },
                "map": {
                    "cardinality": "many",
                    "parent_key": "id",
                    "child_key": "userId"
                }
            },
            "result": "cats"
        }
    ]
]
```

The plan consists of two transactions. Before any transaction is executed, it is assumed that these parameters are supplied to the query: `userId` and `id`.

1. Query the `User` table on the `UserDo` Durable Object to get the `dogId` and `id` fields. Store all results in the root of the output:

EX Output:

```json
{
    "id": 1,
    "dogId": 2
}
```

2. Query the `Dog` and `Cat` tables on the `AnimalDb` D1 database to get the associated `Dog` and `Cat` Models. Store the results in the output under the keys `dog` and `cats`, respectively.

EX Output:

```json
{
    "dog": {
        "id": 2,
        "name": "Fido"
    },
    "cats": [
        {
            "id": 1,
            "name": "Whiskers"
        },
        {
            "id": 2,
            "name": "Fluffy"
        }
    ]
}
```

Any `SELECT` statement will always be ordered by the primary key of the Model being queried.

Results and arguments will follow an accessor format string like `"path.to.field"`, where the first element is always in the root(s) of the output, and each subsequent element is a key in the output object.

#### `LIST User`

LIST will follow a similar plan to GET, but will not require an argument for the primary key of the base model, and will accept a `limit` argument to limit the number of base models returned.

In order to avoid the `N+1` problem, the runtime will always batch queries for related models, and will return an array of results for each related model.

For example, a plan for `LIST User` with `IncludeTree` `{ dog, cats }` would look like this in JSON:

```json
[
    [
        {
            "db": {
                "name": "UserDo",
                "args": {
                    "from_params": ["userId"]
                }
            },
            "sql": {
                "query": "SELECT * FROM User LIMIT ?1 ORDER BY id ASC",
                "args": {
                    "from_params": ["limit"]
                },
                "map": {
                    "cardinality": "many",
                    "parent_key": null,
                    "child_key": null
                }
            },
            "result": ""
        },
    ],
    [
        {
            "db": {
                "name": "AnimalDb",
                "args": []
            },
            "sql": {
                "query": "SELECT * FROM Dog WHERE id IN (?1) ORDER BY id ASC",
                "args": {
                    "from_result": ["dogId"]
                },
                "map": {
                    "cardinality": "many",
                    "parent_key": "dogId",
                    "child_key": "id"
                }
            },
            "result": "dogs"
        },
        {
            "db": {
                "name": "AnimalDb",
                "args": []
            },
            "sql": {
                "query": "SELECT * FROM Cat WHERE userId IN (?1) ORDER BY id ASC",
                "args": {
                    "from_result": ["id"]
                },
                "map": {
                    "cardinality": "many",
                    "parent_key": "id",
                    "child_key": "userId"
                }
            },
            "result": "cats"
        }
    ]
]
```

This plan consists of two transactions. Before any transaction is executed, it is assumed that these parameters are supplied to the query: `userId` and `limit`.

1. Query the `User` table on the `UserDo` Durable Object to get the `dogId` and `id` fields. Store all results in the root of the output:

```json
[
    {
        "id": 1,
        "dogId": 2
    },
    {
        "id": 2,
        "dogId": 3
    }
]
```

2. Query the `Dog` and `Cat` tables on the `AnimalDb` D1 database

Instead of looping through each `User` in the result list and mapping to individual `SELECT` queries, the runtime will batch the queries for `Dog` and `Cat` by 1000 at a time, and return an array of results for each related model. For example, the first batch of `Dog` queries would look like this:

```sql
SELECT * FROM Dog WHERE id IN (2, 3) ORDER BY id ASC
```

If there are more than 1000 `Dog` ids, the runtime will execute another query for the next batch of 1000, and so on until all `Dog` ids have been queried.

EX Output:

```json
[
    {
        "id": 2,
        "dogId": 2,
        "dog": {
            "id": 2,
            "name": "Fido"
        },
        "cats": [
            {
                "id": 1,
                "name": "Whiskers"
            },
            {
                "id": 2,
                "name": "Fluffy"
            }
        ]
    }
]
```

#### R2 and KV

An R2 or KV store can be hydrated in any step along with SQL queries, and will be executed in parallel with any other queries in the same transaction. The results of the R2 or KV query will be stored in the output under the key specified in the plan.

For example, a step to query an R2 bucket would look like this in JSON:

```json
{
    "db": {
        "name": "MyR2Bucket",
        "kind": "r2",
    },
    "query": {
        "key": "my-key/{id}",
        "args": {
            "from_result": ["id"]
        },
    },
    "result": "myR2Object"
}
```

### SAVE Plans

<todo>