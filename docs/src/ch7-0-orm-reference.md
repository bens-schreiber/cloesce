# ORM Reference

> [!WARNING]
> The ORM is subject to change as new features are added.

Cloesce is responsible for hydrating your Models with data from various sources, in the same manner a traditional ORM hydrates objects from data in a database.

Additionally, Cloesce aims to replace boilerplate [CRUD](./ch6-2-crud-generation.md) operations with simple `save`, `get`, and `list` methods on your Models.

The internal tools that Cloesce uses to accomplish this are available for the developer to use as well, allowing you to write custom SQL queries while still leveraging the power of Cloesce's schema.

## Generated Backend ORM

Two namespaces are placed on every Model in the generated backend code:

1. `GeneratedSource`
2. `Orm`

### GeneratedSource

The `GeneratedSource` namespace contains transpiled representations of each [Data Source](./ch5-0-data-sources.md) defined for that Model. For example, the namespace for the Model `WeatherReport`:

```ts
// ...types and impls omitted for brevity
    export namespace GeneratedSource {
        export const Default = {
            tree: { weatherEntries: {} },
            selectQuery: `SELECT ... FROM "WeatherReport" ...`,

            getQuery(env, id: number): D1PreparedStatement {...},
            async get(env, id: number) {...},

            listQuery(env, lastSeen_id: number, limit: number): D1PreparedStatement {...},
            async list(env, lastSeen_id: number, limit: number) {...},

            async save(env, model: DeepPartial<WeatherReport.Self>) {...},
        }
    }
```

| Property / Method       | Description                                                                                                                                                          |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tree`                  | The [Include Tree](./ch5-1-overview.md#include-trees) for this Data Source, specifying all fields to be hydrated when this Data Source is used.                      |
| `selectQuery`           | The transpiled `SELECT` statement (with `LEFT JOIN`s and column aliases) for this Data Source's Include Tree. Useful as a base for custom queries via a CTE.         |
| `getQuery`, `listQuery` | Methods returning a `D1PreparedStatement` (D1) or `SqlStatement` (Durable Object) for the `get` and `list` operations, so you can fetch all properties in one query. |
| `get`                   | A method that retrieves a single instance of the Model using this Data Source. May return `null` if no matching row is found.                                        |
| `list`                  | A method that retrieves multiple instances of the Model using this Data Source. Not available for Models that require route parameters.                              |
| `save`                  | A method that creates or updates a Model instance from a partial object. Returns a fully hydrated instance.                                                          |

> [!TIP]
> Data Source implementations access these via `this`, e.g. `this.selectQuery` and `this.tree`. See [Custom Data Sources](./ch5-2-custom-data-sources.md#include-expansion) for an example.

### Orm

While the `GeneratedSource` namespace exposes methods scoped to a defined Data Source, the `Orm` namespace contains lower level methods for hydrating and mapping Model instances. Each method defaults its Include Tree to the Default Data Source's `tree`.

These methods are especially useful when you need to write custom queries but still want to leverage the schema and hydration capabilities of Cloesce.

| Method    | Parameters                    | Description                                                                                                                                                                                      |
| --------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `save`    | `env`, `newModel`, `include?` | Creates or updates a Model instance from a partial object, using an optional [Include Tree](./ch5-1-overview.md#include-trees) to guide nested relationships. Returns a fully hydrated instance. |
| `get`     | `env`, `{ query?, include? }` | Retrieves a single instance. Accepts an optional `D1PreparedStatement` to fetch D1 properties in the same query. Returns `null` if no matching row is found.                                     |
| `list`    | `env`, `{ query?, include? }` | Retrieves all instances. Accepts an optional `D1PreparedStatement`. Generated only for D1 backed Models.                                                                                         |
| `map`     | `result`, `include?`          | Reconstructs the object graph from a `D1Result` based on the column aliases. Generated only for D1 backed Models. Wraps the static `Orm.map` from the `cloesce` package.                         |
| `hydrate` | `env`, `base`, `include?`     | Takes some base object and fetches any KV or R2 properties to return a fully populated Model instance. Also instantiates objects like Dates and Blobs per the Model definition.                  |

> [!NOTE]
> For Durable Object backed Models, `save`, `get`, and `hydrate` take the injected Durable Object as their first argument (instead of `env`), since hydration must occur within that object's execution context.

More information on how the lower level building blocks work can be found in the [next section](#using-the-base-orm-methods).

## Using the Base ORM Methods

The `Orm` namespace generated on each Model wraps the `Orm` class exported from the `cloesce` package. That class exposes the primitives `select`, `map`, and `hydrate` directly, each backed by an inner WebAssembly implementation. These take the Model's `Meta` (exported on every generated Model) as their first argument.

### select

`Orm.select` generates a SQL `SELECT` statement with `LEFT JOIN`s and column aliases to retrieve all properties of a Model according to an [Include Tree](./ch5-1-overview.md#include-trees).

For example, given the Models Boss, Person, Dog, and Cat, where Boss has many Persons, and Person has one Dog and one Cat:

```cloesce
// ...
model Boss for Db {
    primary {
        id: int
    }

    nav Person::bossId {
        persons
    }
}

source WithAll for Boss {
    include {
        persons {
            dogs
            cats
        }
    }
}

```

`Orm.select(Boss.Meta, null, Boss.GeneratedSource.WithAll.tree)` produces:

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

Note the specific column aliases in the `SELECT` statement. Not only are these valuable to the `map` method, but they can be used in tandem with a common table expression to simplify the process of writing custom SQL queries with complex relationships:

```sql
WITH included AS (
    -- Generated SQL from Orm.select goes here
)
SELECT * FROM included WHERE "persons.dogs.name" = 'Fido'
```

### map

`Orm.map` takes the results of a D1 `SELECT` query and attempts to reconstruct the object graph based on the column aliases. This is useful when you need to write custom SQL but still want to leverage the ORM's hydration capabilities.

```ts
const result = await env.Db.prepare(
  `
    WITH included AS (
        -- Generated SQL from Orm.select goes here
    )
    SELECT * FROM included WHERE "persons.dogs.name" = 'Fido'
`,
).all();

const mapped = Orm.map(Boss.Meta, result, Boss.GeneratedSource.WithAll.tree);
```

While these mapped objects are now full JavaScript objects with the correct relationships, they are not yet hydrated according to the Model definition. For example, if the Model has any KV or R2 fields, those properties will not yet be populated.

### hydrate

`Orm.hydrate` takes some base object (e.g. an element from `map`) and fetches any KV or R2 properties to return a fully populated Model instance. Additionally, it instantiates objects like Dates and Blobs according to the Model definition.

Since the generated `Boss.Orm.hydrate` wrapper already binds the Model's `Meta` and accepts `env` directly, prefer it over the raw `cloesce` package call:

```ts
const mapped = Orm.map(Boss.Meta, result, Boss.GeneratedSource.WithAll.tree);
const hydrated = await Boss.Orm.hydrate(env, mapped[0], Boss.GeneratedSource.WithAll.tree);
```
