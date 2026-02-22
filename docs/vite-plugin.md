# cloesce Vite Plugin

## What it does

The `cloesce/vite` plugin automatically runs `npx cloesce compile` inside your project whenever Vite's dev server starts or detects a file change. This keeps your generated files (worker, client SDK, `wrangler.toml`) in sync with your `.cloesce.ts` definitions without you having to run the compiler manually.

## How to use it

In your project's `vite.config.ts`:

```ts
import { defineConfig } from "vite";
import { cloesce } from "cloesce/vite";

export default defineConfig({
    plugins: [cloesce()],
});
```

That's it. The compiler will now run automatically.

### Restricting which files trigger a recompile

By default, any file change triggers a recompile. If you want to limit it to only changes inside a specific directory (e.g. your data models), pass an `include` array:

```ts
plugins: [cloesce({ include: ["/src/data/"] })]
```

Only file paths containing one of those strings will trigger compilation.

## How it works

The plugin uses two Vite hooks:

### `buildStart`

Runs once when the Vite dev server first starts. It immediately executes `npx cloesce compile` so your generated files are up to date before the browser loads anything.

### `hotUpdate`

Fires every time Vite detects a file change (every save). When triggered:

1. Checks if the changed file matches the `include` filter (skipped if `include` is empty, meaning all files pass).
2. If a compile is already in progress, skips to avoid running two compiles simultaneously.
3. Runs `npx cloesce compile` and logs the output to the Vite dev server's logger.

```
[cloesce] Compiling...         ← hotUpdate fired
[cloesce] Compile completed    ← done, generated files updated
```

If the compile fails (e.g. a syntax error in your `.cloesce.ts`), the error is logged and the dev server keeps running.
