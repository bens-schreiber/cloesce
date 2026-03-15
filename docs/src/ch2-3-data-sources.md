# Data Sources

If you fetch a Model you may notice that Cloesce will leave `undefined` or empty arrays in deeply nested composition with other Models. This is intentional, and is handled by Data Sources.

## What are Data Sources?
> [!IMPORTANT]
> All scalar properties (e.g., `string`, `number`, `boolean`, etc.) are always included in query results. Include Trees are only necessary for Navigation Properties.

Data Sources are Cloesce's response to the overfetching and recursive relationship challenges when modeling relational databases with object-oriented paradigms.

For example, in the Model definition below, how should Cloesce know how deep to go when fetching a Person and their associated Dog?

```typescript
import { Model, Integer } from "cloesce/backend";

@Model("db")
export class Dog {
    id: Integer;

    ownerId: Integer;
    owner: Person | undefined;
}

@Model("db")
export class Person {
    id: Integer;

    dogs: Dog[];
}

// => { id: 1, dogs: [ { id: 1, owner: { id: 1, dogs: [ ... ] } } ] } ad infinitum
```

If we were to follow this structure naively, fetching a `Person` would lead to fetching their `Dog`, which would lead to fetching the same `Person` again, and so on, resulting in an infinite loop of data retrieval.

Data Sources, through their `includeTree` configuration, allow developers to explicitly state which related Navigation Properties should be included in the query results, preventing overfetching. If a Navigation Property is not included in the `includeTree`, it will remain `undefined` (for singular relationships) or an empty array (for collections).

A common convention to follow when writing singular Navigation Properties is to define them as `Type | undefined`, indicating that they may not be populated unless explicitly included.

## Default Data Source

Cloesce will create a default Data Source for each model called "default". This Data Source will include all KV, R2, and D1 properties, but will avoid both circular references and nested relationships in arrays.

For example, in the `Person` and `Dog` Models above, the default Data Source for `Person` would be:
```typescript
{
    includeTree: {
        dogs: {
            // No more includes, preventing infinite recursion
        }
    }
}
```

The default Data Source for `Dog` would be:
```typescript
{
    includeTree: {
        owner: {
            dogs: {
                // No more includes, preventing infinite recursion
            }
        }
    }
}
```

The default Data Source can be generated on demand using the Cloesce ORM (see [Cloesce ORM chapter](./ch2-6-cloesce-orm.md) for more details), or it can be overridden with a custom Data Source definition (see next section).

## Custom Data Sources

In addition to the default Data Source, you can define custom Data Sources on your Models, or even override the default Data Source. Each Data Source you define on a Model will be accessible by the client for querying that Model, and you can have as many Data Sources as you want.

```typescript
import { Model, Integer, DataSource } from "cloesce/backend";

@Model("db")
export class Dog {
    id: Integer;

    ownerId: Integer;
    owner: Person | undefined;
}

@Model("db")
export class Person {
    id: Integer;
    dogs: Dog[];

    static readonly withDogsOwnersDogs: DataSource<Person> = {
        includeTree: {
            dogs: {
                owner: {
                    dogs: {
                        // ... could keep going!
                    }
                }
            }
        }
    };

    static readonly default: DataSource<Person> = {
        includeTree: {}
    };
}
```

In this example, we defined a custom Data Source called `withDogsOwnersDogs` on the `Person` Model. This Data Source specifies that when fetching a `Person`, we want to include their `dogs`, and for each `Dog`, we want to include their `owner`, and for each `owner`, we want to include their `dogs` again. This allows for a much deeper fetch than the default Data Source, but it is still explicitly defined to prevent infinite recursion.

We also overrode the default Data Source for `Person` to be an empty include tree, meaning that by default, fetching a `Person` will not include any related Navigation Properties unless some other Data Source is specified in the query.

## Custom Data Source Queries

On top of creating the structure of hydrated data, Data Sources are also responsible for the underlying SQL queries to fetch that data. Each Data Source comes with two default implementations for the methods: `get` and `list`.

`get` is responsible for fetching a single instance of the Model, while `list` is responsible for fetching multiple instances. `get` can take only the primary key(s) as arguments, while `list` can take `lastSeen`, `limit` and `offset` arguments for pagination.

Each method accepts an argument `joined` which generates `SELECT * FROM ... JOIN ...` query based off the `includeTree` structure of the Data Source.

```typescript

// Cloesce will generate a data source like this by default.
const customDs: DataSource<User> = {
    includeTree: {
        dogs: {}
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

    // Array of parameters available for the list method. Also defines
    // the order of those parameters. `lastSeen` can be multiple primary keys 
    // for composite key models, defined in the same order as the primary keys.
    listParams: ["LastSeen", "Limit"] 
}

@Model("db")
export class Person {
    id: Integer;
    dogs: Dog[];

    static readonly default: DataSource<Person> = customDs;
}
```

See the [Cloesce ORM chapter](./ch2-6-cloesce-orm.md) and [Model Methods chapter](./ch2-5-model-methods.md) for more details on how to use custom Data Sources in queries.