# Building and Migrating

Building a Cloesce project consists of three steps
1. Compilation
2. Running database migrations
3. Building your frontend code

## Compiling

In your project directory, run the following command to compile your Cloesce Models:

```bash
npx cloesce compile
```

This command looks for a `cloesce.config.ts` file in your project root, which contains configuration settings for Cloesce. If the file is not found, or settings are omitted, default values will be used. Unlike many other tools, Cloesce does not require a configuration file to be written in Cloesce itself, which allows you to execute arbitrary code during compilation to generate your schema (e.g. pull environment variables, read from other files, etc).

After compilation, a `.cloesce` folder is created in your project root. This should **not** be committed to source control, as it is regenerated on each build.

| File        | Description |
|-------------|-------------|
| `cidl.json` | The Cloesce Interface Definition Language AST exported to JSON. This file is used internally by Cloesce during migrations, and is utilized by the generated backend code as a source of truth for the structure of your Models and their linked features. |
| `client.ts` | The generated client code for accessing your Models from the frontend. Import this file in your frontend code to interact with your Cloesce Models over HTTP. |
| `backend.ts` | The generated Cloesce ORM and API stubs for your backend. All Cloesce features translate to a namespace or interface in this file. |

## Generating Migrations

To generate database migration files based on changes to your Cloesce Models, run the following command:

```bash
npx cloesce migrate <d1-binding> <migration-name>

# Or to generate a migration for all D1 bindings:
npx cloesce migrate --all <migration-name>
```

This command compares your current Cloesce Models against the last applied migration and generates a new migration file in the `migrations/<d1-binding>` folder with the specified `<migration-name>`. The migration file contains SQL statements to update your D1 database schema to match your Models.

You must apply the generated migrations to your D1 database using the Wrangler CLI:

```bash
npx wrangler d1 migrations apply <d1-binding-name>
```

## Running

After compiling and applying migrations, you can build and run your application locally using Wrangler:

```bash
npx wrangler dev --port <port-number>
```