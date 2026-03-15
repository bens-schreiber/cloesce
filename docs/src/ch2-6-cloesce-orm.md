# Cloesce ORM

> [!CAUTION]
> The ORM is subject to change as new features are added.

During the hydration step of the Cloesce runtime, all of a Model's data is fetched from its various defined sources (D1, KV, R2) and combined into a single object instance. This unified object can then be used seamlessly within your application code.

This functionality is exposed through the `Orm` class in the `cloesce/backend` package.

## Data Sources

A `DataSource<T>` describes how a Model should be fetched and hydrated. It pairs an optional `IncludeTree` (which relationships to join) with optional custom SQL for `get` and `list` queries.

```typescript
interface DataSource<T> {
    includeTree?: IncludeTree<T>;
    get?: (joined: (from?: string) => string) => string;
    list?: (joined: (from?: string) => string) => string;
    listParams?: ("LastSeen" | "Limit" | "Offset")[];
}
```

- `includeTree` — which relationships to include (KV, R2, 1:1, 1:M, M:M).
- `get` — custom SQL for `orm.get`. Receives a helper that generates the joined SELECT. Primary key columns are always bound in order via `?`.
- `list` — custom SQL for `orm.list`. Receives the same helper. Bind parameters are declared in `listParams`.
- `listParams` — which parameters to bind when executing the custom `list` query. Defaults to empty.

All ORM methods that accept an include accept either a `DataSource<T>` or a plain `IncludeTree<T>` interchangeably.

### Default Data Source

Cloesce generates a default `DataSource` for every Model at compile time. It includes all near relationships (KV, R2, 1:1) and the shallow side of 1:M and M:M relationships. This is used whenever no explicit Data Source is provided to an ORM method or instance method.

You can access it at runtime with:

```typescript
const defaultDs = Orm.defaultDataSource(User);
```

Defining a `static readonly default` property on your Model that is a `DataSource<T>` with an `includeTree` overrides the compiler-generated default.

## Getting and Listing Models

```typescript
import { Orm } from "cloesce/backend";
import { User } from "@data"

const orm = Orm.fromEnv(env);
const user = await orm.get(User, {
    primaryKey: { id: 1 },
    keyParams: { myParam: "value" },
    include: User.withFriends
});
// => User | null

const users = await orm.list(User, { include: User.withFriends });
// => User[]
```

`get` requires the primary key via `primaryKey`. For composite primary keys, supply all key columns: `primaryKey: { professorId: 1, courseId: 2 }`. Any `keyParams` needed to construct KV or R2 keys are passed alongside it. Returns `null` when no matching row is found.

`list` takes an optional args object and cannot be used with Models that require key parameters for KV or R2 properties. Use prefix queries for those instead.

### Pagination

`orm.list` uses seek-based pagination by default. Pass `lastSeen`, `limit`, and `offset` to page through results:

```typescript
const page1 = await orm.list(User, { limit: 50 });

const page2 = await orm.list(User, {
    lastSeen: { id: page1[page1.length - 1].id },
    limit: 50
});
```

The default query is `WHERE (primaryKey) > (lastSeen) ORDER BY primaryKey LIMIT ?`, which stays consistent under concurrent inserts. For `LIMIT`/`OFFSET` pagination or custom ordering, provide a `list` function on a custom Data Source.

### Paginated KV and R2 Fields

KV and R2 list fields are declared with `Paginated<T>`:

```typescript
@Model("db")
class User {
    id: Integer;

    @KV("settings/{id}", namespace)
    settings: Paginated<KValue<unknown>>;

    @R2("files/{id}", bucket)
    files: Paginated<R2ObjectBody>;
}
```

```typescript
interface Paginated<T> {
    results: T[];    // first page, up to 1,000 entries
    cursor: string | null;
    complete: boolean;
}
```

To retrieve the next page, use the `cursor` from the previous result with a custom method on your Model.

## Select, Map and Hydrate

When you need filtering, ordering, or aggregation beyond what `get` and `list` provide, write the SQL directly. The ORM gives you three methods to bridge raw SQL results back to hydrated Model instances.

`Orm.select` generates the appropriate `SELECT` with `LEFT JOIN`s and column aliases for a given Data Source. For example, given:

```typescript
@Model()
export class Boss {
    id: Integer;
    persons: Person[];

    static readonly withAll: DataSource<Boss> = {
        includeTree: {
            persons: {
                dogs: {},
                cats: {}
            }
        }
    };
}
```

`Orm.select(Boss, { include: Boss.withAll })` produces:

```sql
SELECT 
    "Boss"."id" AS "id",
    "Person_1"."id" AS "persons.id",
    "Person_1"."bossId" AS "persons.bossId",
    "Dog_2"."id" AS "persons.dogs.id",
    "Dog_2"."personId" AS "persons.dogs.personId",
    "Cat_3"."id" AS "persons.cats.id",
    "Cat_3"."personId" AS "persons.cats.personId"
FROM "Boss"
LEFT JOIN "Person" AS "Person_1" 
    ON "Boss"."id" = "Person_1"."bossId"
LEFT JOIN "Dog" AS "Dog_2" 
    ON "Person_1"."id" = "Dog_2"."personId"
LEFT JOIN "Cat" AS "Cat_3" 
    ON "Person_1"."id" = "Cat_3"."personId"
```

The aliased columns make it straightforward to filter on nested relationships via a CTE:

```typescript
const query = `
    WITH BossCte AS (
        ${Orm.select(Boss, { include: Boss.withAll })}
    )
    SELECT * FROM BossCte
    WHERE [persons.dogs.id] = 5
      AND [persons.cats.id] = 10
      AND [persons.id] = 15
`;
```

An optional `from` string wraps a subquery as the base table:

```typescript
Orm.select(Boss, {
    from: "SELECT * FROM Boss WHERE name = 'Alice'",
    include: Boss.withAll
});
```

Pass the D1 results to `Orm.map` to reconstruct the object graph:

```typescript
const results = await d1.prepare(query).all();
const bosses = Orm.map(Boss, results, Boss.withAll);
// => Boss[]
```

Then `orm.hydrate` fetches any KV and R2 properties and returns fully populated Model instances:

```typescript
const orm = Orm.fromEnv(env);
const hydratedBosses = await orm.hydrate(Boss, {
    base: bosses,
    keyParams: {...},
    include: Boss.withAll
});
// => Boss[]
```

> [!NOTE]
> `Orm.map` requires results in the exact aliased format produced by `Orm.select`. Mixing in results from other queries may fail.


## Saving a Model

`orm.upsert` handles both creating and updating a Model, including nested D1 and KV relationships. R2 properties are not supported; large binary data is better handled separately.

```typescript
import { Orm } from "cloesce/backend";
import { User } from "@data"

const orm = Orm.fromEnv(env);
const result = await orm.upsert(User, {
    // id: 1, omit to auto-increment
    name: "New User",
    friends: [
        { name: "Friend 1" },
        { id: 1, name: "My Best Friend" } // update existing
    ]
}, User.withFriends);
```

The returned instance has all primary keys assigned and any navigation properties specified by the third argument (`DataSource<T>` or `IncludeTree<T>`) populated.