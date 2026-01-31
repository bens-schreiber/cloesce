# Building and Migrating

Building a Cloesce project typically consists of three main steps:
1. Compiling via Cloesce
2. Running database migrations to apply any schema changes to your D1 database.
3. Building your frontend code.

## Compiling

In your project directory, run the following command to compile your Cloesce Models:

```bash
$ npx cloesce compile
```

This command looks for a `cloesce.config.json` file in your project root, which contains configuration settings for the Cloesce compiler. If the file is not found, or settings are omitted, default values will be used.

After compilation, a `.generated` folder is created in your project root. This should __not__ be committed to source control, as it is regenerated on each build. The `.generated` folder contains:
- `cidl.json`: The Cloesce Interface Definition Language file, representing your data Models and their relationships.
- `client.ts`: The generated client code for accessing your Models from the frontend.
- `workers.ts`: The generated Cloudflare Worker handling linking and calling `CloesceApp.init`

Also generated in your project root is a `wrangler.toml` file, which is configured based on your `@WranglerEnv` definition in your code. This file defines your Cloudflare Workers environment, including bindings for D1 databases, KV namespaces, R2 buckets, and environment variables. Cloesce will never overwrite your `wrangler.toml` bindings and configurations, only ensure the bindings defined in your `@WranglerEnv` are present. 

> *Note*: Cloesce does not fully replace using a `wrangler.toml`, high level settings such as account ID, name, and type must still be defined manually.

> *Alpha Note*: `wrangler.jsonc` is not fully supported. Please use `wrangler.toml` for now.

## Generating Migrations

To generate database migration files based on changes to your Cloesce Models, run the following command:

```bash
$ npx cloesce migrate <migration-name>
```

This command compares your current Cloesce Models against the last applied migration and generates a new migration file in the `migrations/` folder with the specified `<migration-name>`. The migration file contains SQL statements to update your D1 database schema to match your Models.

You must apply the generated migrations to your D1 database using the Wrangler CLI:

```bash
$ npx wrangler d1 migrations apply <database-binding-name>
```

## Running

After compiling and applying migrations, you can build and run your Cloudflare Worker locally using Wrangler:

```bash
$ npx wrangler dev --port <port-number>
```