# Building and Migrating

Building a Cloesce project generally consists of three steps:
1. Compilation
2. Running database migrations
3. Building your frontend code

## Compiling

In your project directory, run the following command to compile your Cloesce Models:

```bash
npx cloesce compile
```

This command looks for a `cloesce.config.json` file in your project root, which contains configuration settings for the Cloesce compiler. If the file is not found, or settings are omitted, default values will be used.

After compilation, a `.generated` folder is created in your project root. This should **not** be committed to source control, as it is regenerated on each build. The folder contains:
- `cidl.json`:
    
    The Cloesce Interface Definition Language file, representing your data Models and their relationships. This file is used internally by Cloesce for generating client code, migrations, and running the Cloudflare Worker runtime.

- `client.ts`: 
    
    The generated client code for accessing your Models from the frontend. Import this file in your frontend code to interact with your Cloesce Models over HTTP.

- `workers.ts`: 
    
    The generated Cloudflare Worker code with all linked dependencies (including your custom `main` function if defined). This file is the entry point for your Cloudflare Worker and is referenced in the generated `wrangler.toml`.
    
> *Alpha Note*: `wrangler.jsonc` is not fully supported. Please use `wrangler.toml` for now.

## Generating Migrations

To generate database migration files based on changes to your Cloesce Models, run the following command:

```bash
npx cloesce migrate <migration-name>
```

This command compares your current Cloesce Models against the last applied migration and generates a new migration file in the `migrations/` folder with the specified `<migration-name>`. The migration file contains SQL statements to update your D1 database schema to match your Models.

You must apply the generated migrations to your D1 database using the Wrangler CLI:

```bash
npx wrangler d1 migrations apply <database-binding-name>
```

## Running

After compiling and applying migrations, you can build and run your application locally using Wrangler:

```bash
npx wrangler dev --port <port-number>
```