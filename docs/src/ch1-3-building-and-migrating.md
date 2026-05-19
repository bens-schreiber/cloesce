# Building and Migrating

## Configuration

Before compilation, a  configuration file should be defined in your project root under `cloesce.jsonc`. This file specifies important settings for the Cloesce compiler, such as the paths to your schema files, the URL for your local Workers environment, and the format for generating Wrangler configuration files.

```json
{
  "src_paths": ["./src/schema"],
  "workers_url": "http://localhost:5000/api",
  "wrangler_config_format": "jsonc", // or "toml"
}
```

Multiple configuration files can be defined for different environments (e.g., `staging.cloesce.jsonc`, `production.cloesce.jsonc`). Select the desired configuration file using the `--env` flag when running Cloesce commands:

```bash
cloesce --env staging ...
```

## Compilation

Compilation will generate backend stubs and a frontend API client under the `.cloesce` directory. In your project directory, run the following command to compile your Cloesce schema:

```bash
cloesce compile
```

Generated artifacts should not be modified directly, or committed to source control. Instead, they should be imported and used in your application code.

## Migrations

Cloesce supports any number of D1 databases in a single project. To generate SQL migration files for a specific D1 binding, run the following command:
```bash
cloesce migrate --binding <d1-binding> <migration-name>
```

To generate migrations for all D1 bindings in your project, use the `--all` flag:
```bash
cloesce migrate --all <migration-name>
```

### Applying Migrations

Cloesce will only generate the SQL for migrations. You must apply the generated migrations to your D1 database using the Wrangler CLI:

```bash
npx wrangler d1 migrations apply <d1-binding-name>
```

## Running

After compilation and migrations, run your application locally with Wrangler:

```bash
npx wrangler dev --port <port-number>
```

## Deploying

Deploy your application to Cloudflare's edge with Wrangler

```bash
npx wrangler deploy
```
