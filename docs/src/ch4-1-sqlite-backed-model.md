# SQLite Backed Models

Models are able to pull from various sources of data, including SQLite databases stored in [D1](./ch3-2-d1.md) and [Durable Objects](./ch3-3-durable-objects.md).

When backed by a SQLite database, a Model's properties will translate to a table in that particular database.

## Defining an Environment Binding

To back a Model with a SQLite database, you first need some storage binding that supports SQLite. Two options exist:

```cloesce
// 1. Cloudflare D1
d1 {
    MyDb
}

// 2. Durable Objects
durable MyDurableObject {
    shard {
        tenant: string
    }
}
```

See more information on [D1](./ch3-2-d1.md) and [Durable Objects](./ch3-3-durable-objects.md) definitions in the Environment chapter.

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

model User for MyDurableObject(tenant) {
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

### Across the Stack

Once defined, the `User` Model is a first class citizen across the frontend, backend, and database layers of your application.

For example, the frontend of your application will generate the following TypeScript type for the `User` Model:

```ts
// .cloesce/client.ts
export class User {
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
