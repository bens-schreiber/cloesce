# Proposal: Pagination

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-02-26
- **Last Updated:** 2026-02-26

---

## Summary

Pagination is the act of splitting a large query result set into smaller chunks that can be retrieved incrementally. Cloesce Models, which are backed by D1, KV, and R2, can yield large data sets that are currently returned in their entirety. This proposal outlines a pagination system for Cloesce Models that would allow both developers and clients to retrieve data in smaller chunks.

---

## Motivation

D1, KV, and R2 all support incremental retrieval of data in smaller chunks. Currently, when `orm.list` is called with a Model, it retrieves every D1 row and every KV/R2 value via a prefix list. There is no way for a developer to paginate this data, meaning custom list methods must be implemented with both raw SQL and hydration logic. This is a non-trivial amount of work, and it would be ideal for Cloesce to handle this for developers.

---

## Goals and Non-Goals

### Goals

- Allow pagination of D1 results from both the ORM and the client-generated `LIST` method.
- Allow pagination of KV and R2 results from both the ORM and the client-generated `LIST` method.

### Non-Goals

- Implementing cursor-based pagination for KV and R2 within Cloesce. We will simply expose the cursor and associated metadata to developers, who can then query for the next page of results themselves.
- Nested pagination of relationships (e.g., paginating the `posts` relationship on a `User` model). Developers can implement this themselves with custom methods on their models.
- Validation of SQL in Data Sources (a static analyzer is likely coming in the future).

---

## Design

### KV and R2

Both KV and R2 support cursor-based pagination. After a list query is performed, the response includes a cursor (an opaque string) that can be used to retrieve the next page of results.

Cloesce will make one opinionated assumption here: you always want the maximum number of results on the first page. We will therefore use the default page size for both KV and R2, which is 1,000 results.

In the current implementation, a field containing a list of KV or R2 objects is declared as `field: KValue<unknown>[]` or `field: R2ObjectBody[]` in the model definition. To support pagination, we will introduce a new `Paginated` type to the Cloesce grammar:

```ts
// Shared between backend and client
interface Paginated<T> {
    results: T[];
    cursor: string | null;
    complete: boolean; // true if cursor is null, or if KV indicates list_complete
}
```

In a model definition, you would write `field: Paginated<KValue<unknown>>` or `field: Paginated<R2ObjectBody>` to indicate that a field is paginated. Cloesce will hydrate this type when a query is made, populating `results` with the first page, `cursor` with the token for the next page, and `complete` with whether all results have been retrieved. This type will also be surfaced to the client for use in custom methods.

This grammar replaces the current array syntax, which is a breaking change.

> [!NOTE]
> Sharing a cursor with the client is not particularly dangerous — it is an opaque string that can only be used to retrieve the next page of results, and only if a method exists that exposes that capability.

### D1

Paginating D1 results is less straightforward, as it is a relational database without a native pagination primitive.

SQL pagination can be implemented in two ways:

1. **`LIMIT` / `OFFSET`** — e.g., `SELECT * FROM table LIMIT 10 OFFSET 20` to retrieve the third page with a page size of 10.
2. **Seek method** — e.g., `SELECT * FROM table WHERE id > last_seen_id ORDER BY id LIMIT 10`, which filters from the last seen primary key.

By default, Cloesce will use the seek method on the primary key of a Model. It is more efficient than `OFFSET` and produces more consistent results, particularly when new records are being inserted concurrently.

#### Without a Custom SQL `select` Statement

Default pagination queries will always follow this format:

```sql
SELECT * FROM <all D1 relationships of the IncludeTree>
WHERE <ModelName>.<primaryKey> > ?
ORDER BY <ModelName>.<primaryKey>
LIMIT ?
```

The two bound parameters are the last seen primary key from the previous page and the page size. Ordering by a unique column and filtering from the last seen key ensures stable, consistent pagination regardless of concurrent inserts — unlike `OFFSET`, which can skip or repeat rows as new records are added.

#### Problems with a Custom SQL `select` Statement

The current `select` field in a Data Source accepts a raw SQL string used for both GET and LIST queries. Its integration with pagination is problematic. For example, given a custom statement like:

```sql
SELECT * FROM User ORDER BY User.name WHERE User.name > "greg"
```

The current implementation wraps it as a subquery:

```sql
SELECT * FROM (SELECT * FROM User ORDER BY User.name WHERE User.name > "greg") WHERE User.id = ?
```

A pagination query would then look like:

```sql
SELECT * FROM (
    SELECT * FROM User ORDER BY User.name WHERE User.name > "greg"
) as q
WHERE q.id > ? ORDER BY q.id LIMIT ?
```

This is inefficient — the database must execute the entire inner query before the outer query can filter and sort the results. This approach does not scale well for large data sets.

#### Solution: `get` and `list` Methods on Data Sources

Instead of a single `select` string, developers can define separate `get` and `list` methods on a Data Source. This allows the developer to write a pagination-aware `list` query without Cloesce needing to manipulate it:

```ts
const customDs: DataSource<User> = {
    includeTree: {
        posts: {}
    },
    get: (joined) => `
        WITH joined AS (${joined()})
        SELECT * FROM joined WHERE id = ?
    `,
    list: (joined) => `
        WITH joined AS (${joined()})
        SELECT * FROM joined WHERE id > ? ORDER BY id LIMIT ?
    `
}
```

Both methods receive a `joined` parameter — a function that returns the SQL joining all relationships defined in `includeTree`. The `get` method binds a single `id` parameter, while the `list` method binds the last seen `id` and the page size, in that order.

> [!NOTE]
> A `Paginated` type is not necessary for D1, as only the root level of a query can be paginated.

---

## Implementation

### KV and R2

The Cloesce grammar will be updated to include the `Paginated` type. The hydration logic will be updated to populate `results`, `cursor`, and `complete` when a list query is made. The client-generated `LIST` method will be updated to return `Paginated<T>` instead of `T[]`. The `Paginated` interface will be added to both the backend runtime and the client TypeScript output via the Handlebars template.

### D1

The ORM will be updated to accept an optional `lastSeen` parameter and a `limit` parameter for list queries. When `lastSeen` is not provided, the ORM will use Model metadata to supply a sensible default (e.g., `0` for integer primary keys). `limit` will default to `1000` but can be overridden by the caller.

Within the ORM, the `get` and `list` functions defined on a Data Source take precedence. If neither is defined, the ORM falls back to the default seek-method query described above.

Parameter bindings are applied in the order `lastSeen`, `limit`, `offset`. This is safe because SQL grammar enforces a fixed clause order — `WHERE` always precedes `LIMIT`, which always precedes `OFFSET` — so bound parameters will always appear in this sequence regardless of how the query is written.

### LIST CRUD Method

If a Model has a D1 table associated with it, the generated `LIST` method will be updated to accept optional pagination parameters (`lastSeen`, `limit`, and `offset`).

---

## Example

```ts
@Model()
class Post {
    id: Integer;
    title: string;

    userId: Integer;
}

@Model(["LIST"])
class User {
    id: Integer;
    name: string;

    posts: Post[];

    @KV("settings/{id}", namespace)
    settings: Paginated<KValue<unknown>>;

    @R2("files/{id}", bucket)
    files: Paginated<R2ObjectBody>;

    static readonly orderedByName: DataSource<User> = {
        includeTree: {
            posts: {}
        },
        get: (joined) => `
            WITH joined AS (${joined()})
            SELECT * FROM joined WHERE id = ?
        `,
        list: (joined) => `
            WITH joined AS (${joined()})
            SELECT * FROM joined ORDER BY name LIMIT ? OFFSET ?
        `
    }
}
```

```ts
const usersByName = await orm.list(User, {
    include: User.orderedByName,
    limit: 3000,
    offset: 1000,
});

const usersById = await orm.list(User, {
    include: undefined, // use default include tree
    lastSeen: 100,
    limit: 1000,
});

// Iterating through KV results beyond the first page using the cursor
const someUser = usersByName[0];
let result = await env.namespace.list({ cursor: someUser.settings.cursor });
while (!result.list_complete) {
    // process result.keys
    result = await env.namespace.list({ cursor: result.cursor });
}
```