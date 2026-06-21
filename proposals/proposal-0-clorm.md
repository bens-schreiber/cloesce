# Proposal: Cloesce ORM v2

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 06-20-2026
- **Last Updated:** 06-20-2026

---

## Summary

---

## Motivation

Imagine the case where we are making a Reddit clone, where every `User`,  `SubReddit`, `Post` and `Comment` are different durable objects.

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
- Any Model should be able to have a relationship with any other Model (be it either `1:1`, `1:N` or both).

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


### 1:1 Relationships

#### D1 -> Worker
```cloesce
model WorkerBacked {
    route {
        routeId: int
    }
}

model D1Backed for Db {
    primary {
        primaryId: int
    }

    one WorkerBacked::routeId(primaryId) {
        workerBacked
    }
}
```

Here, `D1Backed` is capable of having a `1:1` relationship with `WorkerBacked`. In order to resolve the relationship, the ORM would have to first fetch `D1Backed` from D1, then use it's `primaryId` field to hydrate the `WorkerBacked` model.

This relationship `1:1` relationship is possible because all `route` fields are supplied in the constructor.

#### Worker -> D1

```cloesce
model WorkerBacked {
    route {
        routeId: int
    }

    one D1Backed::primaryId(routeId) {
        d1Backed
    }
}

model D1Backed for Db {
    primary {
        primaryId: int
    }
}
```

In the reverse case, `WorkerBacked` has a `1:1` relationship with `D1Backed`. To resolve the relationship, the ORM would first have to fetch `WorkerBacked`, then use its `routeId` field to construct a SQLite query to fetch the `D1Backed` model.

This `1:1` relationship is possible because all `primary` fields are supplied in the constructor.

#### D1 -> DO

```cloesce
model DoBacked for Do(tenantId) {
    route {
        routeId: int
    }
}

model D1Backed for Db {
    primary {
        primaryId: int
        tenantId: int
    }

    one DoBacked::{tenantId(tenantId), routeId(primaryId)} {
        doBacked
    }
}
```

`D1Backed` has a `1:1` relationship with `DoBacked`. To resolve the relationship, the ORM would first have to fetch `D1Backed`, then use its `tenantId` to construct the DO id, then use its `primaryId` to hydrate a `DoBacked` models `routeId` field.

This `1:1` relationship is possible because all shard fields (`tenantId`) and `route` fields (`routeId`) are supplied in the constructor.

#### DO -> D1

```cloesce
model DoBacked for Do(tenantId) {
    route {
        routeId: int
    }

    one D1Backed::primaryId(routeId) {
        d1Backed
    }
}

model D1Backed for Db {
    primary {
        primaryId: int
    }
}
```

This example is much like the `Worker -> D1` example, where the DO model has a `1:1` relationship with the D1 model. To resolve the relationship, the ORM would first have to fetch `DoBacked`, then use its `routeId` field to construct a SQLite query to fetch the `D1Backed` model.

#### DO A -> DO B

```cloesce
model DoBackedA for DoA(tenantId) {
    route {
        routeId: int
    }

    one DoBackedB::{tenantId(tenantId), routeId(routeId)} {
        doBackedB
    }
}

model DoBackedB for DoB(tenantId) {
    route {
        routeId: int
    }
}
```

Like the `D1 -> DO` example, `DoBackedA` has a `1:1` relationship with `DoBackedB`. To resolve the relationship, the ORM would first have to fetch `DoBackedA`, then use its `tenantId` to construct the DO id of `DoBackedB`, then use its `routeId` to hydrate the `DoBackedB` model.

#### D1 A -> D1 B

```cloesce
model D1BackedA for DbA {
    primary {
        primaryId: int
    }
    
    column {
        bId: int
    }

    one D1BackedB::primaryId(bId) {
        d1BackedB
    }
}

model D1BackedB for DbB {
    primary {
        primaryId: int
    }
}
```

`D1BackedA` has a `1:1` relationship with `D1BackedB`. To resolve the relationship, the ORM would first have to fetch `D1BackedA`, then use its `bId` field to construct a SQLite query to fetch the `D1BackedB` model.

This requires two separate queries to two separate databases, but is possible because all `primary` fields are supplied in the constructor.


### 1:N Relationships

Several relationships will always be impossible with `1:N`, because not all Models can be indexed. Any `Worker` backed model (be it with no backing or a `DO` that just uses `route` fields) will never be able to have a `1:N` relationship with any other model, because there is no way to index it.

#### Worker -> D1

```cloesce
model WorkerBacked {
    route {
        routeId: int
    }

    many D1Backed::primaryId(routeId) {
        d1Backed
    }
}

model D1Backed for Db {
    primary {
        primaryId: int
    }
}
```

In this example, `WorkerBacked` has a `1:N` relationship with `D1Backed`. To resolve the relationship, the ORM would first have to fetch `WorkerBacked`, then use its `routeId` field to construct a SQLite query to fetch all `D1Backed` models that have a matching `primaryId`.

#### D1 -> DO

```cloesce
model D1Backed for Db {
    primary {
        primaryId: int
        tenantId: int
    }

    many DoBacked::{tenantId(tenantId), primaryId(primaryId)} {
        doBacked
    }
}

model DoBacked for Do(tenantId) {
    primary {
        primaryId: int
    }
}
```

In this example, `D1Backed` has a `1:N` relationship with `DoBacked`. To resolve the relationship, the ORM would first have to fetch `D1Backed`, then use its `tenantId` to construct the DO id of `DoBacked`, then finally make a SQLite query to fetch all `DoBacked` models that have a matching `primaryId`.

#### DO A -> DO B

```cloesce
model DoBackedA for DoA(tenantId) {
    primary {
        primaryId: int
    }

    many DoBackedB::{tenantId(tenantId), primaryId(primaryId)} {
        doBackedB
    }
}

model DoBackedB for DoB(tenantId) {
    primary {
        primaryId: int
    }
}
```

### Unindexed Relationships

#### Missing Discriminators

In some cases, a Model may have no `primary` key or `route` fields, and therefore cannot be indexed by some key.

The syntax for this Model (which would only be capable of `1:1` relationships) would look like this:

```cloesce
model UnindexedModel { }

model IndexedModel for Db {
    primary {
        primaryId: int
    }

    one UnindexedModel {
        unindexedModel
    }
}
```

#### No Discriminator Provided

It may be useful to have a `1:N` relationship where `N` is simply the entire collection of a Model. This would only be possible in a SQLite backed Model, and would look like this:

```cloesce
model Post for Db {
    primary {
        id: int
    }
}

model User for Db {
    primary {
        id: int
    }

    many Post {
        posts
    }
}
```

#### Only Durable Shard Discriminators Provided

Like in the above case, it may be useful to have a `1:N` relationship where `N` is the entire collection of a Model, but this time for a DO backed Model. Since Cloesce still needs to know which DO to query, the shard fields would still need to be provided.

```cloesce
model Post for Do(tenantId) {
    primary {
        id: int
    }
}

model User for Do(tenantId) {
    primary {
        id: int
        tenantId: int
    }

    // This fetches ALL posts for a given tenantId, no other discriminator provided.
    many Post::{tenantId(tenantId)} {
        posts
    }
}
```

### Example: Reddit Clone

Using the syntax described above, we can now express the relationships between our `User`, `SubReddit`, `Post` and `Comment` models in a Reddit clone.

To demonstrate a small example, assume that:
- `UserDo`, `SubRedditDo`, `PostDo`, and `CommentDo` are all Durable Objects
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

In this example, each `User` Model has the tables `UserFollowedSubReddit`, `UserComment` and `UserPost` stored in SQLite, and each one of them has a `1:1` relationship with their respective Model.

Cloesce would be able to hydrate this entire relationship, allowing the backend to only focus on business logic and not have to worry about how to fetch related data across multiple DOs.