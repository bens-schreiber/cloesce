# A basic D1 Model

In this section, we will create a simple Cloesce Model backed by a Cloudflare [D1 database](https://developers.cloudflare.com/d1/). This Model will represent a `User` entity with basic properties such as `id` and `name`. 

We will also explore how to define the Model, its properties, and how to use the generated CRUD methods.

## Defining the User Model

Compilation in Cloesce is composed of three phases: Extraction, Analysis and Code Generation. 

During Extraction, the compiler scans your source files (designated with `*.cloesce.ts`) for Model definitions. Models are defined using the `@Model()` decorator. 

```typescript
import { Model, Integer, PrimaryKey } from "cloesce/backend";

@Model()
export class User {
    @PrimaryKey
    id: Integer;

    name: string;
}
```

The above code defines a Model "User" with several properties:
| Property | Description |
|--------|-------------|
| `User` | Cloesce infers from the class attributes that this model is backed by a D1 table `User` |
| `id` | Integer property decorated with `@PrimaryKey`, indicating it is the model’s primary key. |
| `name` | String property representing the user’s name; stored as a regular column in the D1 database. |

> *TIP*: Using the `@PrimaryKey` decorator is optional if your primary key property is named `id` or `<className>Id` (in any casing, ie snake case, camel case, etc). The compiler will automatically treat a property named `id` as the primary key.

## Supported D1 Column Types

Cloesce supports a variety of column types for D1 Models. Here are some of the most commonly used types:

| TypeScript Type | SQLite Type | Notes |
|-----------------|-------------|-------|
| `Integer` | `INTEGER` | Represents an integer value |
| `string` | `TEXT` | Represents a string value |
| `boolean` | `INTEGER` | 0 for false, 1 for true |
| `Date` | `TEXT` | Stored in ISO 8601 format |
| `number` | `REAL` | Represents a floating-point number |
| `Uint8Array` | `BLOB` | Represents binary data |

All of these types by themselves are `NOT NULL` by default. To make a property nullable, you can use a union with `null` e.g., `property: string | null;`. `undefined` is reserved for navigation properties.

Notably, an `Integer` primary key is automatically set to `AUTOINCREMENT` in D1, so you don't need to manually assign values to it when creating new records (useful for the [ORM functions](./ch2-6-cloesce-orm.md)).

## Migrating the Database

The standard Cloesce compilation command does not perform database migrations. To create or update the D1 database schema based on your Model definitions, you need to run the migration command:

```bash
$ npx cloesce compile # load the latest Model definitions
$ npx cloesce migrate <migration name>
```

> *TIP*: Any change in a D1 backed Model definition (adding, removing or modifying properties, renaming Models, etc) requires a new migration to be created. The migration command will generate a new migration file in the `migrations/` directory.