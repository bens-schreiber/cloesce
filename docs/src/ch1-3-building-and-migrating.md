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

All keys are optional except `src_paths`, which tells the compiler where to find your `.clo` files:

| Key                      | Default                   | Description                                                       |
| ------------------------ | ------------------------- | ----------------------------------------------------------------- |
| `src_paths`              | `[]`                      | Directories searched for `.clo` schema files.                     |
| `out_path`               | `".cloesce"`              | Directory for generated artifacts (`cidl.json`, backend, client). |
| `workers_url`            | `"http://localhost:8787"` | Base URL the generated client sends requests to.                  |
| `migrations_path`        | `"./migrations"`          | Directory where generated SQL migrations are written.             |
| `wrangler_config_format` | `"toml"`                  | Format of the generated Wrangler config: `"toml"` or `"jsonc"`.   |

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
> Schema modifications to a [SQLite backed Model](./ch4-2-sqlite-backed-model.md) should be accompanied by a new migration. This ensures that your database schema stays in sync with your Cloesce Models.

Migrations turn a Cloesce schema into a set of SQL statements that can be applied to a database, tracking changes over time.

Each migration is written to `<migrations_path>/<binding>/`, and one of `--binding` or `--all` is required.

**Specific Binding**

```bash
cloesce migrate --binding <binding> <migration-name>
```

**All Bindings**

```bash
cloesce migrate --all <migration-name>
```

### Apply D1 Migrations

Cloesce generates the SQL for migrations, but does not apply them.

If a [D1 database](./ch3-2-d1.md) is being utilized, a `.sql` file is generated, which you must apply using the Wrangler CLI:

```bash
npx wrangler d1 migrations apply <binding-name>
```

### Apply Durable Object Migrations

A [Durable Object's](./ch3-3-durable-objects.md) SQLite storage is not reachable from the Wrangler CLI, so its migrations are generated as `.ts` modules instead of `.sql` files.

Import each one and pass it to `durable`. They are applied once, in order, before the Durable Object serves any request:

```ts
import { createApp, CfEnv } from "@cloesce/backend.js";
import initMigration from "../migrations/MyDo/1785712992_init.js";

export class MyDo extends DurableObject<CfEnv> {
  private base = createApp().durable(this, [initMigration]);

  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}
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
