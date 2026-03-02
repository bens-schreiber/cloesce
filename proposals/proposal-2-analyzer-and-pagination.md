# Proposal: Pagination

- **Author(s):** Ben Schreiber
- **Status:** Draft | Review | Accepted | Rejected | **Implemented**
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

Paginating D1 results is less straightforward, because it lacks a native pagination primitive like a `cursor`. SQL pagination can be implemented in two ways:

1. **`LIMIT` / `OFFSET`** — e.g., `SELECT * FROM table LIMIT 10 OFFSET 20` to retrieve the third page with a page size of 10.
2. **Seek method** — e.g., `SELECT * FROM table WHERE id > last_seen_id ORDER BY id LIMIT 10`, which filters from the last seen primary key.

The `LIMIT` / `OFFSET` method can lead to inconsistent results if the rows are being concurrently inserted or deleted, because the offset is applied after the result set is generated. However, it is the best way to implement pagination when ordering by a non-unique column (e.g., `ORDER BY name`), because the seek method requires a unique, sequential column to filter from.

The seek method is more performant and consistent for large data sets, because it can take advantage of indexes and doesn't require the database to count or skip rows. It requires a unique, sequential column (usually the primary key) to filter from.

For Cloesce’s purposes, we will support both methods. By default, the ORM will use the seek method with the primary key for pagination. Custom data sources can be used to implement pagination with `LIMIT` / `OFFSET` if the developer wants to paginate by a non-unique column or use a different pagination strategy.

#### Default Pagination with the Seek Method

Default pagination queries will always follow this format:

```sql
SELECT * FROM <all D1 relationships of the IncludeTree>
WHERE <ModelName>.<primaryKey> > ?
ORDER BY <ModelName>.<primaryKey>
LIMIT ?
```

The two bound parameters are the last seen primary key from the previous page and the page size. Ordering by a unique column and filtering from the last seen key ensures stable, consistent pagination regardless of concurrent inserts.

#### `get` and `list` Methods on Data Sources

Instead of a single `select` string, developers can define separate `get` and `list` methods on a Data Source. This allows the developer to write a pagination-aware `list` query without Cloesce needing to manipulate it:

```ts
interface DataSource<T> {
    includeTree?: IncludeTree;
    get?: (joined: () => string) => string;
    list?: (joined: () => string) => string;
    listParams?: ("lastSeen" | "limit" | "offset")[]
}

const customDs: DataSource<User> = {
    includeTree: {
        posts: {}
    },

    // NOTE: This is equivalent to the default `get` implementation
    get: (joined) => `
        WITH joined AS (${joined()})
        SELECT * FROM joined WHERE id = ?
    `,

    // NOTE: This is equivalent to the default `list` implementation
    list: (joined) => `
        WITH joined AS (${joined()})
        SELECT * FROM joined WHERE id > ? ORDER BY id LIMIT ?
    `,
    listParams: ["lastSeen", "limit"]
}
```

Note that we also need to specify the parameter names in `listParams` so that Cloesce can bind them correctly. If `listParams` are not provided, an empty array is assumed and no bindings can be utilized.

`get` is assumed to accept a single parameter for the primary key.


> [!NOTE]
> A `Paginated` type is not necessary for D1, as only the root level of a query can be paginated.

---

## Implementation

### KV and R2

The Cloesce grammar will be updated to include the `Paginated` type. The hydration logic will be updated to populate `results`, `cursor`, and `complete` when a list query is made. The client-generated `LIST` method will be updated to return `Paginated<T>` instead of `T[]`. The `Paginated` interface will be added to both the backend runtime and the client TypeScript output via the Handlebars template.

### D1

The ORM `list` method will be updated to accept an arguments struct:
```ts
interface ListArgs {
    lastSeen?: unknown; // type depends on the primary key type
    limit?: number;
    offset?: number;
}
```

When `list` is called, e.g., `orm.list(User)`, the ORM will check if the `User` model has a custom `list` method on its Data Source. If it does not, it will assume we are using the default seek method for pagination, using the `lastSeen` and `limit` parameters from `ListArgs` (with defaults if they are not provided) to bind the query parameters. 

If a custom `list` method is defined, the ORM will bind parameters from the names specified in `listParams` (or `lastSeen` and `limit` by default) to the query in the order they are defined. If a parameter is not provided by the caller, the ORM will use a default value (`0` for `lastSeen`, `1000` for `limit`, and `0` for `offset`) if the parameter is expected by the query.

### LIST CRUD Method

All `LIST` methods generated for the client will be updated to accept the same arguments struct as the ORM `list` method, allowing clients to also take advantage of pagination in their queries. 

Note that this CRUD method will not validate input other than ensuring the parameters are of the correct type (e.g., `number` for `limit`). If a developer wanted to limit pagination size or enforce that `lastSeen` is provided, they would need to implement a custom method on their model that performs those checks and then calls `orm.list` with the appropriate parameters.
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

        // NOTE: If the parameters appeared many times in the query, we could use positional
        // parameters (e.g., $1, $2), which uses the order of parameters in listParams to bind them correctly.
        list: (joined) => `
            WITH joined AS (${joined()})
            SELECT * FROM joined ORDER BY name LIMIT ? OFFSET ?
        `,
        listParams: ["limit", "offset"]
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