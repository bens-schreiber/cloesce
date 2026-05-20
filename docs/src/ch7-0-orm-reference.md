# ORM Reference

> [!WARNING]
> The ORM is subject to change as new features are added.

Cloesce is responsible for hydrating your Models with data from various sources, in the same manner a traditional ORM hydrates objects from data in a database.

Additionally, Cloesce aims to replace boilerplate [CRUD](./ch6-2-crud-generation.md) operations with simple `save`, `get`, and `list` methods on your Models.

The internal tools that Cloesce uses to accomplish this are available for the developer to use as well, allowing you to write custom SQL queries while still leveraging the power of Cloesce's schema.

## Generated Backend ORM

Three namespaces are placed on every Model in the generated backend code:

1. `Source`
2. `Orm`
3. `Key`

### Source

The `Source` namespace contains transpiled representations of each [Data Source](./ch5-0-data-sources.md) defined for that Model. For example, the namespace for the Model `WeatherReport`:

```ts
// ...types and impls omitted for brevity
    export namespace Source {
        export const Default = {
            include: {"weatherEntries":{}},

            async save(env, newModel: DeepPartial<WeatherReport>) {...},

            getQuery: (env, id: number) => ...,
            async get(env, id: number) {...},

            listQuery: (env, lastSeen_id: number, limit: number) => ...,
            async list(env, lastSeen_id: number, limit: number),
        }
    }
```

| Property / Method       | Description                                                                                                                                                               |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `include`               | The [Include Tree](./ch5-1-overview.md#include-trees) for this Data Source, which specifies all fields to be included when this Data Source is used.                      |
| `save`                  | A method that creates or updates a Model instance from a partial object. Returns a fully hydrated instance.                                                               |
| `get`                   | A method that retrieves a single instance of the Model using this Data Source, given the necessary parameters. May return `null` if no matching row is found.             |
| `list`                  | A method that retrieves multiple instances of the Model using this Data Source, given the necessary parameters. Not available for Models that require key parameters.     |
| `getQuery`, `listQuery` | Methods that return a `D1PreparedStatement` for the `get` and `list` operations, which can be used to fetch D1 properties in the same query as the base Model properties. |

### Orm

While the `Source` namespace contains methods for interacting with a Model according to the defined Data Sources, the `Orm` namespace contains lower level methods for hydrating and mapping Model instances.

These methods are especially useful when you need to write custom queries but still want to leverage the schema and hydration capabilities of Cloesce.

| Method    | Parameters                   | Description                                                                                                                                                                                                                                                              |
| --------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `save`    | `env`, `instance`, `include` | Creates or updates a Model instance from a partial object, using some [dynamic Include Tree](./ch5-1-overview.md#include-trees) for guidance in nested relationships. Returns a fully hydrated instance.                                                                 |
| `get`     | `env`, `query`               | Retrieves a single instance. Optionally, accepts a `D1PreparedStatement` to fetch any D1 properties in the same query. Returns `null` if no matching row is found.                                                                                                       |
| `list`    | `env`, `query`               | Retrieves all instances. Optionally, accepts a `D1PreparedStatement` to fetch any D1 properties in the same query. Not compatible with Models that require key parameters for KV or R2 properties.                                                                       |
| `select`  | `include`, `from`            | Generates a SQL `SELECT` statement with the necessary `LEFT JOIN`s and column aliases to retrieve all properties of a Model according to an [Include Tree](./ch5-1-overview.md#include-trees). Optionally, accepts a `from` string to wrap a subquery as the base table. |
| `map`     | `result`, `include`          | Takes the results of a `SELECT` query and reconstructs the object graph based on the column aliases. This is useful when you need to write custom SQL but still want to leverage the ORM's hydration capabilities.                                                       |
| `hydrate` | `env`, `base`, `include`     | Takes some base object and fetches any KV or R2 properties to return a fully populated Model instance. Additionally, it instantiates objects like Dates and Blobs according to the Model definition.                                                                     |

More information on how these methods work can be found in the [next section](#using-the-base-orm-methods).

### Key

The `Key` namespace contains methods to easily generate keys for KV and R2 fields based on the Model's schema. For example:

```cloesce
model Weather {
    // ...

    r2 (bucket, "weather/photos/{id}.jpg") {
        photo
    }
}

```

```ts
export namespace Weather {
  // ...
  export namespace Key {
    export function photo(id: number): string {
      return `weather/photos/${id}.jpg`;
    }
  }
}
```

## Using the Base ORM Methods

The methods in the `Orm` namespace are all available on the generated backend Model, but can also be used directly from the `Orm` class in the `cloesce` package.

Each method in the `Orm` class has an inner implementation in WebAssembly.

### select

`Orm.select` generates a SQL `SELECT` statement with `LEFT JOIN`s and column aliases to retrieve all properties of a Model according to an [Include Tree](./ch5-1-overview.md#include-trees).

For example, given the Models Boss, Person, Dog, and Cat, where Boss has many Persons, and Person has one Dog and one Cat:

```cloesce
// ...
model Boss {
    primary {
        id: int
    }

    nav (Person::bossId) {
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

`Orm.select(Boss.Meta, null, Boss.Source.WithAll.include)` produces:

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
SELECT * from included WHERE [persons.dogs.name] = 'Fido'
```

### map

`Orm.map` takes the results of a D1 `SELECT` query and attempts to reconstruct the object graph based on the column aliases. This is useful when you need to write custom SQL but still want to leverage the ORM's hydration capabilities.

```ts
const result = await env.DB.prepare(
  `
    WITH included AS (
        -- Generated SQL from Orm.select goes here
    )
    SELECT * from included WHERE [persons.dogs.name] = 'Fido'
`,
).all();

const mapped = Orm.map(Boss.Meta, result, Boss.Source.WithAll.include);
```

While these mapped objects are now full JavaScript objects with the correct relationships, they are not yet hydrated according to the Model definition. For example, if the Model has any KV or R2 fields, those properties will not yet be populated.

### hydrate

`Orm.hydrate` takes some base object (e.g. from `map`) and fetches any KV or R2 properties to return a fully populated Model instance. Additionally, it instantiates objects like Dates and Blobs according to the Model definition.

```ts
const mapped = Orm.map(Boss.Meta, result, Boss.Source.WithAll.include);
const hydrated = await Orm.hydrate(env, mapped, Boss.Source.WithAll.include);
```
