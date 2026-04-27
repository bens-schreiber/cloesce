# Testing

Cloesce Models and Services live as isolated units with no inherent connection to an incoming Worker request, making them ideal for unit testing.

To write tests for Cloesce that utilize any ORM features, ensure you have:

- Ran `cloesce compile` to generate the necessary files
- Applied migrations for the Models being tested
- Invoked `cloesce` to initialize the Cloesce runtime

```typescript
import { cloesce } from "@cloesce/backend";
beforeAll(() => cloesce());
```

Cloesce needs only the CIDL (generated during compilation) to be used in tests. This means you can write tests for your Models and Services without needing to run a full Cloudflare Worker environment.

ORM methods rely on Cloudflare Workers bindings (D1, KV, R2, etc.), so you will need to mock these bindings in your test environment. The best choice for this is [Miniflare](https://developers.cloudflare.com/workers/testing/miniflare/), which provides an in memory implementation of Cloudflare Workers runtime and bindings.

A basic Miniflare setup is included in the template project which can be installed with:

```bash
npx create-cloesce my-cloesce-app
```
