# SQLite Backed Models

A Model can be backed by a SQLite database, stored in either a [D1](./ch3-2-d1.md) database or a [Durable Object](./ch3-3-durable-objects.md).

## Defining an Environment Binding

To back a Model with a SQLite database, you first need some [storage binding](./ch3-0-environment.md) that supports SQLite.

```cloesce
// Cloudflare D1
d1 {
    MyDb
}

// Durable Object
durable MyDurableObject {
    shard {
        tenant: string
    }
}
```

## Defining a Model

### With D1

```cloesce
d1 {
    MyDb
}

model User for MyDb {
    primary {
        id: int
    }

    column {
        name: string
    }
}
```

The above code defines a Model "User" stored in the D1 database `MyDb`, with several properties:

| Property | Description                       |
| -------- | --------------------------------- |
| `User`   | A table in the D1 database `MyDb` |
| `id`     | Integer primary key column        |
| `name`   | String column                     |

### With Durable Objects

```cloesce
durable MyDurableObject {
    shard {
        tenant: string
    }
}

model User for MyDurableObject::tenant {
    primary {
        id: int
    }

    column {
        name: string
    }
}
```

The above code defines a Model "User" stored in the Durable Object `MyDurableObject`, with several properties:

| Property | Description                                                                                                |
| -------- | ---------------------------------------------------------------------------------------------------------- |
| `User`   | A table in the `MyDurableObject` Durable Object's SQLite storage                                           |
| `id`     | Integer primary key column                                                                                 |
| `name`   | String column                                                                                              |
| `tenant` | The shard key used to determine which Durable Object instance the data is stored in. Not stored in SQLite. |

> [!TIP]
>
> You may alias shard keys to any name in the Model declaration:
>
> ```cloesce
> model User for MyDurableObject::tenant(alias) {
>   // ...
> }
> ```
>
> If there are multiple shard keys, any number of them can be aliased:
>
> ```cloesce
> model User for MyDurableObject::{tenant, org(alias)} {
>   // ...
> }
> ```

> [!TIP]
> Just because a Model is backed by a Durable Object does not mean it uses the Durable Object's SQLite storage.
>
> A `column`, `primary` or `foreign` field must be defined for the Model to be represented as a table in SQLite.
>
> For example, the `Gnat` Model from the previous chapter could be backed by a Durable Object:
>
> ```cloesce
> model Gnat for MyDurableObject::tenant {
>     route {
>         id: int
>         buzzing: bool
>     }
> }
> ```
>
> `Gnat`'s fields are still ephemeral, existing for the duration of a request. However, it is tied to an instance of a Durable Object, which will be created based on the `tenant` shard key.

### Across the Stack

Once defined, the `User` Model is a first class citizen across the frontend, backend, and database layers of your application.

For example, the backend of your application will generate the following TypeScript type for the `User` Model:

```ts
// .cloesce/backend.ts
export interface User {
  id: number;
  name: string;

  // iff backed by a Durable Object
  tenant: string;
}
```

In SQLite, the `User` Model will be represented as a table:

```sql
CREATE TABLE User (
    id INTEGER PRIMARY KEY,
    name TEXT
);
```
