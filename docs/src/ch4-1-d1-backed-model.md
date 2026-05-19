# D1 Backed Model

[Cloudflare D1](https://developers.cloudflare.com/d1/) is a distributed SQL database built on SQLite for Workers. Cloesce provides first class support for D1, allowing you to define Models backed by D1 tables with just a few lines of code.

## Defining an Environment Binding

To back a Model with a D1 database, you first need to define an environment binding for the database in your Cloesce schema. This is done using the `env` block, where you specify the D1 databases your application will use.

```cloesce
env {
    d1 {
        my_db
    }
}
```

In the above example, we have defined a D1 environment binding called `my_db`. This binding will be used to reference the D1 database in our Model definitions. Cloesce will generate all necessary Wrangler configurations and typed backend code to seamlessly integrate this D1 database into your application.

See more on environment bindings in the [Environment chapter](./ch3-0-environment.md).

## Defining a Model

> [!TIP]
> Any top level declaration in Cloesce is global across any file in the project. This means that Models declared in one file can be referenced and used in any other file.

The most important aspect of a Model is the data it represents. Models in Cloesce enable "_Data Driven Programming_", where your data model is the source of truth for your entire application.

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

    name: string
}
```

The above code defines a Model "User" stored in the D1 database `my_db`, with several properties:

| Property | Description                        |
| -------- | ---------------------------------- |
| `User`   | A table in the D1 database `my_db` |
| `id`     | Integer field, primary key column  |
| `name`   | String field, regular column       |

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

## Migrating the Database

> [!IMPORTANT]
> Any change in a D1 backed Model definition (adding, removing, or modifying properties; renaming Models) requires a new migration to be created.
>
> The migration command will generate a new migration file in the `migrations/` directory.

The standard Cloesce compilation command does not perform database migrations. To create or update the D1 database schema based on your Model definitions, you need to run the migration command:

```bash
cloesce compile
cloesce migrate --binding <d1-binding> <migration-name>
```

Finally, these generated migrations must be applied to the actual D1 database using Wrangler:

```bash
npx wrangler d1 migrations apply <d1-binding>
```
