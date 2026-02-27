# Proposal: Pagination

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-02-19
- **Last Updated:** 2026-02-21

---

## Summary

Pagination is the act of splitting a large set of data yielded from a query into smaller chunks that can be retrieved incrementally. Cloesce Models, which are backed by D1, KV, and R2 can yield large data sets that are currently naively returned in their entirety. This proposal outlines a pagination system for Cloesce Models that would allow both the developer and clients to retrieve data in smaller chunks.

---

## Motivation

D1, KV, and R2 all have methods of pagination that allow retrieving data incrementally in smaller chunks. Currently, when `orm.list` is called with some Model, it retrieves every D1 row and KV/R2 value via a prefix list. We expose no way for a developer to paginate this data, meaning you must implement your own list method with both custom SQL and hydration logic. This is a non trivial amount of work, and it would be ideal for Cloesce to handle this for developers.


---

## Goals and Non-Goals

### Goals

- Allow pagination of D1 results from both the ORM and the client generated `LIST` method.
- Allow pagination of KV and R2 results from both the ORM and the client generated `LIST` method.

### Non-Goals
- Implementing cursor based pagination for KV and R2 in Cloesce. We will simply expose the cursor and associated metadata to developers, who can then query for the next page of results themselves.
- Nested pagination of relationships, (e.g., paginating the `posts` relationship on a `User` model). Developers can implement this themselves with custom methods on their models.
- Validation of SQL in Data Sources (a static analyzer is likely a future Proposal)

---

## Design

### KV and R2

Both KV and R2 have cursor based pagination. After a list query is performed, the response includes a "cursor" (a string) that can be used to retrieve the next page of results. 

Cloesce is going to assume something bold here: you always want the maximum amount of results on the first page. That means, we will use the default page size for both KV and R2, which is 1000 results.

In the current implementation, indicating that a field is a list of KV or R2 objects is done by writing `field: KValue<unknown>[]` or `field: R2ObjectBody[]` in the model definition. In order to support pagination, we will instead introduce a new type called `Paginated`, which will be added to the Cloesce grammar.

```ts
// On both the backend and client
interface Paginated<T> {
    results: T[];
    cursor: string | null;
    complete: boolean; // true if cursor is null or if KV says so
}
```

In a model definition, you would write `field: Paginated<KValue<unknown>>` or `field: Paginated<R2ObjectBody>` to indicate that this field is paginated.

This `Paginated` type would be hydrated by Cloesce when a query is made, and it would include the first page of results, the cursor for the next page, and a boolean indicating whether there are more results to retrieve. Additionally, it would be sent to the client, who may need to use it to retrieve more results in some custom method.

This grammar would replace the current array syntax, which is a breaking change.

> [!NOTE]
> Sharing a cursor to the client is not particuarly dangerous, as the cursor is just an opaque string that the client can use to retrieve the next page of results (if some method even exposed the capacity to do so).

### D1

D1 is less trivial to paginate results for, as it is a relational database that does not have pagination as a primitive concept.

In SQL, pagination can be done in two ways:
1. Using `LIMIT` and `OFFSET` (e.g., `SELECT * FROM table LIMIT 10 OFFSET 20` to get the 3rd page of results with a page size of 10)
2. Using a "seek method" with a unique, sequential column (e.g., `SELECT * FROM table WHERE id > last_seen_id ORDER BY id LIMIT 10`)

By default, Cloesce will use the seek method for the primary key of a Model, as it is more efficient and leads to more consistent results (especially if new records are being added to the database).


#### Without a custom SQL `select` statement

Default pagination queries will always follow the exact same format:
```sql
SELECT * FROM <all D1 relationships of the IncludeTree> WHERE <ModelName>.<primaryKey> > ? ORDER BY <ModelName>.<primaryKey> LIMIT ?
```

The two parameters would be the last id from the previous page, and the page size. It's important that we:
- Order by a unique column (always the primary key) to ensure consistent pagination results
- Filter by the last id from the previous page to ensure we are getting the next page of
results (better than using OFFSET, which can lead to inconsistent results if there are new records being added to the database)
- Limit by the page size to ensure we are only getting a specific number of results per page

#### Problems with a Custom SQL `select` statement

The current `select` statement in a Data Source is a raw SQL string used for both GET and LIST queries. It's integration into Cloesce is naive, especially in light of pagination. For example, assume we have some custom `select` statement like this:
```sql
SELECT * FROM User ORDER BY User.name WHERE User.name > "greg"
```

The current implementation will treat a custom `select` as a subquery, wrapping queries like this (for a GET):
```sql
SELECT * FROM (SELECT * FROM User ORDER BY User.name WHERE User.name > "greg") WHERE User.id = ?
```

It would then follow that a pagination query would look like this:
```sql
SELECT * FROM (
    SELECT * FROM User ORDER BY User.name WHERE User.name > "greg"
) as q
WHERE q.id > ? ORDER BY q.id LIMIT ?
```

This approach is inefficient, as it requires the database to execute the entire custom query and then filter and sort the results in memory. This is not ideal, especially for large data sets.

#### Solution: `get` and `list` methods on Data Sources

Instead of a single `select` statement, we can allow developers to define two seperate SQL statements: one for `get` and one for `list`. Then, instead of trying to mark up the SQL, we can assume the developer has taken pagination into account when writing their `list` statement. For example:

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

Here, `customDs` defines a `get` method for retrieving a single user by id, and a `list` method for retrieving a paginated list of users.

Both methods take a `joined` parameter (the SQL string that joins all relationships in the defined `includeTree`). Further, the `get` method takes a binded parameter for the id, and the `list` method takes binded parameters for the last id and page size.

---

> [!NOTE]
> A `Paginated` type is not necessary for D1, as only the root level of a query can be paginated.

### Implementation

### KV and R2 

For KV and R2, we would need to update the Cloesce grammar to include the `Paginated` type, and then update the hydration logic to populate the `results`, `cursor`, and `complete` fields when a query is made. We would also need to update the client generated `LIST` method to return a `Paginated` type instead of an array.

The `Paginated` interface would then need to be added to both the backend, and client TypeScript (via the handlebars template).

### D1

The ORM will have to be updated to accept a last seen id and page size parameter for list queries. The last seen id can be nullable, utilizing the Model metadata to fill in a sensible default value (e.g., 0 for integers). The page size can default to some value (e.g., 1000), and can be overridden by the client.

Within the ORM implementation, we would default to the defined functions in the Data Source. If not defined, a default `get` and `list` function would be used, with the `list` function following the pagination format outlined above.

### LIST Crud Method

If a Model has some D1 table associated with it, then the generated `LIST` method would need to be updated to accept optional pagination parameters.

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

// ex: iterate through KV using a cursor
const someUser = usersByName[0];
let result = await env.namespace.list({ cursor: someUser.settings.cursor });
while (!result.list_complete) {
  // process result.keys
  result = await env.namespace.list({ cursor: result.cursor });
}
```

> [!NOTE]
> The implementation will apply parameter bindings in the order: `lastSeen`, `limit`, `offset`. This is safe because a SQL query will always follow this order, due to the grammar of SQL (`WHERE` comes before `LIMIT`, which comes before `OFFSET`).


