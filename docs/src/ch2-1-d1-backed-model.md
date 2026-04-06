# Basic D1 Backed Model

[Cloudflare D1]((https://developers.cloudflare.com/d1/)) is a serverless SQL database built on SQLite for Workers. Cloesce provides first class support for D1, allowing you to define Models backed by D1 tables with just a few lines of code.

## Defining a Model

> [!NOTE] 
> In `v0.3.0`, all symbols within a file are globally scoped, so you can split your Models and APIs across multiple files as you see fit.

All Cloesce models are defined within a `.clo` or `.cloesce` file. 

```cloesce
// my-model.clo
[use db]
model User {
    primary {
        id: int
    }

    name: string
}
```

The above code defines a Model "User" stored in the D1 database `db`, with several properties:
| Property | Description |
|--------|-------------|
| `User` | A table backed by the D1 database `db` |
| `id` | Integer property decorated scoped as a primary key |
| `name` | String property representing the user’s name; stored as a regular column in the D1 database. |

## Supported D1 Column Types

Cloesce supports a variety of column types for D1 Models. These are the supported TypeScript types and their corresponding SQLite types:

| TypeScript Type | SQLite Type | Notes |
|-----------------|-------------|-------|
| `int` | `INTEGER` | Represents an integer value |
| `string` | `TEXT` | Represents a string value |
| `bool` | `INTEGER` | 0 for false, 1 for true |
| `date` | `TEXT` | Stored in ISO 8601 format |
| `double` | `REAL` | Represents a floating-point number |
| `blob` | `BLOB` | Represents binary data |

All of these types by themselves are `NOT NULL` by default. To make a property nullable, you may wrap it in an `Option` generic:
```cloesce
model User {
    optionalField: Option<string>
}
```

Notably, an `int` primary key is automatically set to `AUTOINCREMENT` in D1, so you don't need to manually assign values to it when creating new records (useful for the [ORM functions](./ch2-6-cloesce-orm.md)).

## Migrating the Database

> [!IMPORTANT]
> Any change in a D1 backed Model definition (adding, removing, or modifying properties; renaming Models) requires a new migration to be created. 
>
> The migration command will generate a new migration file in the `migrations/` directory.

The standard Cloesce compilation command does not perform database migrations. To create or update the D1 database schema based on your Model definitions, you need to run the migration command:

```bash
npx cloesce compile # load the latest Model definitions
npx cloesce migrate <d1-binding> <migration name>
```

Finally, these generated migrations must be applied to the actual D1 database using the Wrangler CLI:

```bash
npx wrangler d1 migrations apply <d1-binding>
```