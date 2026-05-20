# D1 Backed Model

[Cloudflare D1](https://developers.cloudflare.com/d1/) is a distributed SQL database built on SQLite for Workers.

Cloesce provides first class support for D1, allowing you to define Models backed by D1 tables with just a few lines of code.

## Defining an Environment Binding

To "back" a Model with a D1 database, you first need to define an environment binding:

```cloesce
env {
    d1 {
        my_db
    }
}
```

See more on environment bindings in the [Environment chapter](./ch3-0-environment.md).

## Defining a Model

> [!TIP]
> Any top level declaration in Cloesce is global across any file in the project. This means that Models declared in one file can be referenced and used in any other file.

Models can pull from various sources of data, including D1 databases, KV namespaces, R2 buckets, and more. In this section, we will focus on defining a D1 backed Model.

### Schema Example

```cloesce
// my-model.clo
env {
    d1 {
        my_db
    }
}

[use my_db]
model User {
    primary {
        id: int
    }

    column {
        name: string
    }
}
```

The above code defines a Model "User" stored in the D1 database `my_db`, with several properties:

| Property | Description                        |
| -------- | ---------------------------------- |
| `User`   | A table in the D1 database `my_db` |
| `id`     | Integer primary key column         |
| `name`   | String column                      |

### Across the Stack

Once defined, the `User` Model is a first class citizen across the frontend, backend, and database layers of your application.

For example, the frontend of your application will generate the following TypeScript type for the `User` Model:

```ts
// .cloesce/client.ts
export class User {
  id: number;
  name: string;
}
```
