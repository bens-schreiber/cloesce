# Building and Migrating

Building a Cloesce project consists of three steps
1. Compilation
2. Running database migrations
3. Building your frontend code

## Cloesce Config

A cloesce config may be defined in a `cloesce.jsonc` file in your project root. This file is used to configure various aspects of the Cloesce compiler and generated code, such as source paths for your schema, the output directory for generated files, and the format of the generated Wrangler configuration file.
```jsonc
{
    "src_paths": ["./src/schema"],
    "workers_url": "http://localhost:5000/api",
    "wrangler_config_format": "jsonc" // or "toml"
}
```


If you have multiple environments (e.g., staging, tests, production), you can define multiple config files by prefixing the name: `staging.cloesce.jsonc`, `production.cloesce.jsonc`, etc. Then, specify which environment to use when running the CLI command:
```bash
cloesce --env staging ...
```

## Compiling

In your project directory, run the following command to compile your Cloesce Models:

```bash
cloesce compile
```

This command looks for a `cloesce.jsonc` file in your current directory, which contains configuration settings for Cloesce. If the file is not found, or settings are omitted, default values will be used. Unlike many other tools, Cloesce does not require a configuration file to be written in Cloesce itself, which allows you to execute arbitrary code during compilation to generate your schema (e.g. pull environment variables, read from other files, etc).

After compilation, a `.cloesce` folder is created in your project root. This should **not** be committed to source control, as it is regenerated on each build.

| File        | Description |
|-------------|-------------|
| `cidl.json` | The Cloesce Interface Definition Language AST exported to JSON. This file is used internally by Cloesce during migrations, and is utilized by the generated backend code as a source of truth for the structure of your Models and their linked features. |
| `client.ts` | The generated client code for accessing your Models from the frontend. Import this file in your frontend code to interact with your Cloesce Models over HTTP. |
| `backend.ts` | The generated Cloesce ORM and API stubs for your backend. All Cloesce features translate to a namespace or interface in this file. |

## Generating Migrations

To generate database migration files based on changes to your Cloesce Models, run the following command:

```bash
cloesce migrate <d1-binding> <migration-name>

# Or to generate a migration for all D1 bindings:
cloesce migrate --all <migration-name>
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