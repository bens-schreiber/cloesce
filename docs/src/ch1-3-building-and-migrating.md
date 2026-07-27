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
> Multiple configuration files can be defined for different environments:
>
> - `<name>.cloesce.jsonc`
>
> Select the desired configuration file using `--env <name>` when running Cloesce commands:
>
> ```bash
> # given `staging.cloesce.jsonc` exists
> cloesce --env staging ...
> ```

## Compilation

In your root directory, run the following command to compile your schema:

```bash
cloesce compile
```

> [!IMPORTANT]
> Any generated artifacts should not be modified directly or committed to source control.
>
> Import them into your backend and client code, relying on a `cloesce compile` build step to keep up to date with your schema.

## Migrations

> [!TIP]
> Schema modifications to a [SQLite backed Model](./ch4-1-sqlite-backed-model.md) should be accompanied by a new migration. This ensures that your database schema stays in sync with your Cloesce Models.

Migrations turn a Cloesce schema into a set of SQL statements that can be applied to a database, tracking changes over time.

**Specific Binding**

```bash
cloesce migrate --binding <binding> <migration-name>
```

**All Bindings**

```bash
cloesce migrate --all <migration-name>
```

### Apply D1 Migrations

Cloesce generates the SQL for migrations, but does not apply them,

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
