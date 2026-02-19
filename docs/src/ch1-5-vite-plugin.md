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

> **Note:** Matching is a simple substring check on the full file path. For example, `["/data/"]` would also match `/metadata/file.ts`.

### `watchDirs`

By default, the plugin watches `src/data` for changes. If your `.cloesce.ts` files live outside Vite's root, add those directories here so Vite's watcher picks them up:

```ts
plugins: [cloesce({ watchDirs: ["../shared/data", "src/data"] })]
```

An empty array disables the extra watchers entirely.

## Behaviour

- **On server start** — runs `npx cloesce compile` once before the browser loads anything.
- **On file change** — runs `npx cloesce compile` each time Vite detects a save. If a compile is already in progress it is skipped to avoid overlapping runs.
- **On error** — compile errors are logged and the server keeps running.

Output during file changes is logged through Vite's built-in logger. Output during the initial build-start compile is logged through Rollup's plugin warning channel (`this.warn`):

```
[cloesce] Compiling...
[cloesce] Compile completed
```
