# ORM Reference

> [!CAUTION]
> The ORM is subject to change as new features are added.

During the hydration step of the Cloesce runtime, all of a [Model's](./ch4-0-models.md) data is fetched from its various defined sources ([D1](./ch4-1-d1-backed-model.md), [KV](./ch4-4-kv-fields.md), [R2](./ch4-5-r2-fields.md)) and combined into a single object instance. This unified object can then be used seamlessly within your application code.

This functionality is exposed in the generated backend code, as well as the `cloesce` library's `Orm` class.

## Generated Backend ORM

When you define a Model in Cloesce, three methods are generated for interacting with it: `get`, `list`, and `save`.

- `save`: Creates or updates a Model instance from a partial object, using some [Include Tree](./ch5-1-overview.md#include-trees) for guidance in nested relationships. Returns a fully hydrated instance.

- `get`: Retrieves a single instance. Optionally, accepts a `D1PreparedStatement` to fetch any D1 properties in the same query. Returns `null` if no matching row is found.

- `list`: Retrieves all instances. Optionally, accepts a `D1PreparedStatement` to fetch any D1 properties in the same query. Not compatible with Models that require key parameters for KV or R2 properties.

While `save` is to be used in most scenarios, `get` and `list` are available for queries that require more complex application logic. For most cases, you can achieve the same result with a [custom Data Source](./ch5-2-custom-data-sources.md).

For example, given the following Cloesce Model:

```cloesce
model User {
    primary {
        id: int
    }

    name: string
}

source ByName for User {
    include { }

    sql get(name: string) {
        "SELECT * FROM User WHERE name = $name"
    }

    sql list(lastName: string, limit: int) {
        "SELECT * FROM User WHERE name > $lastName ORDER BY name LIMIT $limit"
    }
}
```

The generated backend code will create a method `User.Source.ByName` with `get` and `list` functions that execute the defined SQL and return hydrated Model instances.

Always accessible is the [Default Data Source](./ch5-1-overview.md#default-data-source) (`User.Source.Default`), which provides basic `get` and `list` methods without any custom SQL.

When implementing a Cloesce Model, these generated methods are placed directly on the Model:

```ts
const User = clo.User.impl({});

User.Source.ByName.get(env, "Alice");
User.Source.Default.get(env, 1);
```

## Advanced ORM Usage

Internally, Cloesce uses the `CloesceOrm` class from the `cloesce` package to implement the generated methods described above. You may use it directly, or use the generated methods in the namespace of each backend Model, which are more convenient:

```ts
const User = clo.User.impl({});
User.hydrate(env, base);
User.map(result);
User.select();
```

### select

`CloesceOrm.select` generates a SQL `SELECT` statement with the necessary `LEFT JOIN`s and column aliases to retrieve all properties of a Model according to an [Include Tree](./ch5-1-overview.md#include-trees).

For example, given the Model

```cloesce
model Boss {
    primary {
        id: int
    }

    // ...
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

`CloesceOrm.select(Boss.Meta, null, Boss.Source.WithAll.include)` produces:

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

`select` can also take a `from` string to wrap a subquery as the base table:

```typescript
CloesceOrm.select(
  Boss.Meta,
  "SELECT * FROM Boss WHERE name = 'Alice'",
  Boss.Source.WithAll.include,
);
```

### map

`CloesceOrm.map` takes the results of a `SELECT` query and reconstructs the object graph based on the column aliases. This is useful when you need to write custom SQL but still want to leverage the ORM's hydration capabilities.

### hydrate

`CloesceOrm.hydrate` takes a base set of Model instances (e.g. from `map`) and fetches any KV or R2 properties to return fully populated Model instances. Additionally, it instantiates objects like Dates and blobs according to the Model definition.
