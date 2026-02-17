# Vite Plugin

The `cloesce/vite` plugin automatically runs `cloesce compile` whenever your Vite dev server starts or a file changes. This keeps your generated files in sync with your `.cloesce.ts` definitions without running the compiler manually.

## Setup

In your project's `vite.config.ts`, import and add the plugin:

```ts
import { defineConfig } from "vite";
import { cloesce } from "cloesce/vite";

export default defineConfig({
    plugins: [cloesce()],
});
```

The compiler will now run on server start and on every file save.

## Options

### `include`

By default, any file change triggers a recompile. To restrict compilation to changes in a specific directory, pass an `include` array of path patterns:

```ts
plugins: [cloesce({ include: ["/src/data/"] })]
```

Only files whose paths contain one of the provided strings will trigger compilation. An empty array (the default) matches all files.

## Behaviour

- **On server start** — runs `npx cloesce compile` once before the browser loads anything.
- **On file change** — runs `npx cloesce compile` each time Vite detects a save. If a compile is already in progress it is skipped to avoid overlapping runs.
- **On error** — compile errors are logged to the Vite dev server output and the server keeps running.

Output is logged through Vite's built-in logger:

```
[cloesce] Compiling...
[cloesce] Compile completed
```
