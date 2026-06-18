# Building and Migrating

## Configuration

Define a `cloesce.jsonc` file in your project root to configure the Cloesce compiler:

```json
{
  "src_paths": ["./src/schema"],
  "workers_url": "http://localhost:5000/api",
  "wrangler_config_format": "jsonc" // or "toml"
}
```

> [!TIP]
> Multiple configuration files can be defined for different environments (e.g., `staging.cloesce.jsonc`, `production.cloesce.jsonc`). 
>
> Select the desired configuration file using the `--env` flag when running Cloesce commands:
>
> ```bash
> cloesce --env staging ...
> ```

## Compilation

Compilation will transform your Cloesce schema into backend stubs and a client side API under the `.cloesce` directory. In your root directory, run the following command to compile your schema:

```bash
cloesce compile
```

> [!IMPORTANT]
> Any generated artifacts should not be modified directly or committed to source control. Simply import them into your backend and client code, relying on a build step to run the Cloesce compiler and keep the generated code up to date.

## Migrations

> [!TIP]
> Schema modifications to a [SQLite backed Model](./ch4-1-sqlite-backed-model.md) should be accompanied by a new migration. This ensures that your database schema stays in sync with your Cloesce Models.

Cloesce supports any number of SQLite databases in a single project. To generate SQL migration files for a specific binding, run the following command:

```bash
cloesce migrate --binding <binding> <migration-name>
```

To generate migrations for all bindings in your project, use the `--all` flag:

```bash
cloesce migrate --all <migration-name>
```

### Apply D1 Migrations

Cloesce generate the SQL for migrations, but not apply them,

If a [D1 database](./ch3-2-d1.md) is being utilized, you must apply the generated migrations using the Wrangler CLI:

```bash
npx wrangler d1 migrations apply <binding-name>
```

## Running

After compilation and migrations, run your application locally with Wrangler:

```bash
npx wrangler dev --port <port-number>
```

## Deploying

Deploy your application to Cloudflare's edge with Wrangler:

```bash
npx wrangler deploy
```
