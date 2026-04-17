# Data Sources

If you fetch a Model you may notice that Cloesce will leave `undefined` or empty arrays in deeply nested composition with other Models. This is intentional, and is handled by Data Sources.

## What are Data Sources?
> [!IMPORTANT]
> All scalar properties (e.g., `string`, `int`, `bool`, etc.) are always included in query results. Include Trees are only necessary for Navigation Fields.

Data Sources are Cloesce's response to the overfetching and recursive relationship challenges when modeling relational databases with object-oriented paradigms.

For example, in the Model definition below, how should Cloesce know how deep to go when fetching a Person and their associated Dog?

```cloesce
[use db]
model Dog {
    primary {
        id: int
    }

    foreign (Person::id) {
        ownerId
        nav { owner }
    }
}

[use db]
model Person {
    primary {
        id: int
    }

    nav (Dog::ownerId) {
        dogs
    }
}

// => { id: 1, dogs: [ { id: 1, owner: { id: 1, dogs: [ ... ] } } ] } ad infinitum
```

If we were to follow this structure naively, fetching a `Person` would lead to fetching their `Dog`, which would lead to fetching the same `Person` again, and so on, resulting in an infinite loop of data retrieval.

Data Sources, through their `include` block, allow developers to explicitly state which related Navigation Fields should be included in the query results, preventing overfetching. If a Navigation Field is not included in the `include`, it will remain `undefined` (for singular relationships) or an empty array (for collections).

## Default Data Source

Cloesce will create a default Data Source for each model called `Default`. This Data Source will include all KV, R2, and D1 properties, but will avoid both circular references and nested relationships in arrays.

For example, in the `Person` and `Dog` Models above, the default include for `Person` would be:
```cloesce
include {
    dogs 
}
```

The default Data Source for `Dog` would be:
```cloesce
include {
    owner {
        dogs
    }
}
```

## Custom Data Sources

In addition to the default Data Source, you can define custom Data Sources on your Models, or even override the default Data Source. Each Data Source you define on a Model will be accessible by the client for querying that Model, and you can have as many Data Sources as you want.

```cloesce
[use db]
model Dog {
    primary {
        id: int
    }

    foreign (Person::id) {
        ownerId
        nav { owner }
    }
}

[use db]
model Person {
    primary {
        id: int
    }

    nav (Dog::ownerId) {
        dogs
    }
}

source WithDogsOwnersDogs for Person {
    include {
        dogs {
            owner {
                dogs {
                    // ... could keep going!
                }
            }
        }
    }
}

// Override the default to be empty
source Default for Person {
    include {}
}
```

In this example, we defined a custom Data Source called `WithDogsOwnersDogs` on the `Person` Model. This Data Source specifies that when fetching a `Person`, we want to include their `dogs`, and for each `Dog`, we want to include their `owner`, and for each `owner`, we want to include their `dogs` again. This allows for a much deeper fetch than the default Data Source, but it is still explicitly defined to prevent infinite recursion.

We also overrode the default Data Source for `Person` to be an empty include tree, meaning that by default, fetching a `Person` will not include any related Navigation Fields unless some other Data Source is specified in the query.

## Custom Data Source Queries

On top of creating the structure of hydrated data, Data Sources are also responsible for the underlying SQL queries to fetch that data. Each Data Source comes with two default implementations for the methods: `get` and `list`.

`get` is responsible for fetching a single instance of the Model, while `list` is responsible for fetching multiple instances. Both queries may take any parameters. A default implementation is provided for both methods.
- `get` defaults to fetching a single instance based on the primary key(s) of the Model.
- `list` defaults to fetching multiple instances based on a cursor-based pagination strategy using the primary key(s) of the Model.

All parameters are properly bound to prevent SQL injection.

All queries have access to the `$include` parameter, which returns a SQL select statement that joins all the tables with respect to the Data Sources `include` tree. `$include` aliases all columns in a object oriented fashion.

```cloesce
source Custom for Person {
    include {
        dogs
    }

    sql get(id: int) {
        "
            WITH included AS ($include)
            SELECT * FROM included WHERE id = $id
        "
    }

    sql list(lastSeen: int, limit: int) {
        "
            WITH included AS ($include)
            SELECT * FROM included WHERE id > $lastSeen ORDER BY id LIMIT $limit
        "
    }
}
```

Every Data Source will be generated to the backend as a type safe query. By default, all Data Sources are exposed to the client for querying. However, should a Data Source be only intended for internal use, it can be marked `internal`:
```cloesce

internal source MyInternal for Person {
    // ...
}
```

Data Sources are used extensively in the Cloesce ORM and Cloesce API layers to provide flexible and efficient data retrieval. By defining custom Data Sources, developers can optimize their queries to fetch only the necessary data, while still maintaining the ability to easily include related data when needed.

See the [Cloesce ORM chapter](./ch2-6-cloesce-orm.md) and [Model Methods chapter](./ch2-5-model-methods.md) for more details on how to use custom Data Sources in queries.