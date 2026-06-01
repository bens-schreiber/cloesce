# Custom Data Sources

## Motivation

Currently, Cloesce supports inline SQL queries for fetching data from a SQLite database. However, many applications will want application level data source algorithms that are not expressed in solely SQL, such as requiring specific permission checks and caching strategies.

To support this, we will:
1. Ditch SQL binding expansion and code-generation entirely
2. Support "custom data source stubs" that can be implemented in the runtime layer and called from the Cloesce Router

## Design

### Removing SQL Expressions

The first step is to remove the SQL expression syntax from the language. This means that instead of writing:

```cloesce
model User {
    ...
}

source MyDataSource for User {
    include {
        ...
    }

    sql get(userId: int) {
        "
        SELECT * FROM users WHERE id = $userId
        "
    }

    sql list(offset: int, limit: int) {
        "
        SELECT * FROM users LIMIT $limit OFFSET $offset
        "
    }
}
```

The data source can simply be defined as:

```cloesce
model User {
    ...
}

source MyDataSource for User {
    include {
        ...
    }

    get(userId: int)
    
    list(offset: int, limit: int)

    save(user: partial<User>)
}
```

The implementation of these methods will be provided by the runtime, which can use any data fetching strategy it wants. Still however will the Default Data Source and its default implementations need to be available to the runtime, so that a `get`, `list` or Data Source can be omitted and the runtime can fall back to the default implementation.

For example, if `get` is omitted from the schema in `MyDataSource`, the runtime will use the default implementation as it does now (a primary key based fetch from D1).

`get` and `list` would be capable of injecting any parameters just as API methods do now, ex:

```cloesce
source MyDataSource for User {
    [inject Db]
    get(userId: int)
}
```

Additionally, the `save` method will be able to be overriden to support custom save logic.

### Generating and Implementing Custom Data Source Stubs

To support this, we will generate "stubs" for each custom data source method defined in the schema. For example, to implement `MyDataSource` as defined above, the application code would look like this:

```ts
const User = clo.User.impl({
    MyDataSource: {
        async get(include, userId) {
            // Custom implementation for fetching a user by ID
        }

        async list(include, offset, limit) {
            // Custom implementation for fetching a paginated list of users
        }

        async save(tree, user) {
            // Custom implementation for saving a user
        }
    }
});

// useable from within the codebase as
const user = await User.MyDataSource.get(123);
```

Any `get` or `list` implementation would be capable of returning an `HttpResult` to short-circuit the request.

Each method would also receive a `include` parameter, which is of the type:

```ts
interface Include {
    tree: IncludeTree;
    query : string;
}
```

where `query` is the result of the Cloesce ORM's `select` expansion for the given include tree `tree`. This will be precomputed by the compiler so that the runtime doesn't have to invoke the WASM ORM to compute the select query for the include tree.

Note that `save` methods would not receive the `query` parameter, just the `tree`.

For example, the default implementation of the methods would be (assuming `[inject Db]` is used to inject a database instance and that User is backed by Db):
```ts
const User = clo.User.impl({
    MyDataSource: {
        async get(include, env, userId) {
            const query = env.Db.prepare(include.query);
            const result = await this.Orm.get(env, { query, include: include.tree });
            if (!result) {
                return HttpResult.fail(404, "User not found");
            }

            return HttpResult.ok(result.data);
        }

        async list(include, offset, limit) {
            const query = env.Db.prepare(include.query);
            const result = await this.Orm.list(env, { query, include: include.tree });
            return result.data;
        }

        async save(tree, user) {
            const result = await this.Orm.save(env, user, tree);
            return result.data;
        }
    }
});
```

This implementation is exactly what the `CRUD` methods for each data source already do under the hood, but a custom implementation allows for a developer to modify it however they please.
