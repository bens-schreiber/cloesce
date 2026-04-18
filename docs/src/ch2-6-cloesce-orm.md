# Cloesce ORM

> [!CAUTION]
> The ORM is subject to change as new features are added.

During the hydration step of the Cloesce runtime, all of a Model's data is fetched from its various defined sources (D1, KV, R2) and combined into a single object instance. This unified object can then be used seamlessly within your application code.

This functionality is exposed in the generated backend code, as well as the `cloesce` libraries `Orm` class.

## Generated Backend ORM

When you define a Model in Cloesce, three methods are generated for interacting with it `get`, `list`, and `save`.

- `save`: Creates or updates a Model instance from a partial object, using some Include Tree for guidance in nested relationships. Returns a fully hydrated instance.

- `get`: Retrieves a single instance. Optionally, accepts a `D1PreparedStatement` to fetch any D1 properties in the same query. Returns `null` if no matching row is found.

- `list`: Retrieves all instances. Optionally, accepts a `D1PreparedStatement` to fetch any D1 properties in the same query. Not compatible with Models that require key parameters for KV or R2 properties.

While `save` is to be used in most scenarios, `get` and `list` are available for queries that require more complex application logic. For most cases, you can achieve the same result with a custom Data Source.

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

The generated backend code will create a method `User.Sources.ByName` with `get` and `list` functions that execute the defined SQL and return hydrated Model instances.

Always accessible is the Default Data Source (`User.Sources.Default`), which provides basic `get` and `list` methods without any custom SQL.

When implementing a Cloesce Model, these generated methods are placed directly on to the model:
```ts
const User = clo.User.impl({});

User.ByName.get("Alice");
User.Default.get(1);
```

## Advanced ORM Usage

Internally, Cloesce uses the `Orm` class from the `cloesce` package to implement the generated methods described above. You may use it directly, or use the generated methods in the namespace of each backend Model, which are more convenient:

```ts
const User = clo.User.impl({});
User.Orm.hydrate(...);
User.Orm.map(...);
User.Orm.select(...);
```

### select

`Orm.select` generates a SQL `SELECT` statement with the necessary `LEFT JOIN`s and column aliases to retrieve all properties of a Model according to an Include Tree.

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

`Orm.select(Boss.Meta, null, Boss.DataSources.WithAll.include)` produces:

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
Orm.select(Boss.Meta, "SELECT * FROM Boss WHERE name = 'Alice'", Boss.DataSources.WithAll.include);
```

### map

`Orm.map` takes the results of a `SELECT` query and reconstructs the object graph based on the column aliases. This is useful when you need to write custom SQL but still want to leverage the ORM's hydration capabilities.

### hydrate

`Orm.hydrate` takes a base set of Model instances (e.g. from `map`) and fetches any KV or R2 properties to return fully populated Model instances. Additionally, it instantiates objects like Dates and blobs according to the Model definition.