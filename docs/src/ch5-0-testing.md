# Testing

Cloesce Models and Services live as isolated units with no inherent connection to an incoming Worker request, making them ideal for unit testing.

To write tests for Cloesce that utilize any ORM features, ensure you have:

- Ran `npx cloesce compile` to generate the necessary files
- Applied migrations for the Models being tested
- Invoked `CloesceApp.init` to initialize the Cloesce runtime

```typescript
import { CloesceApp } from "cloesce/backend";
import { cidl, constructorRegistry } from "@generated/workers";

// Cloesce must be initialized before utilizing any ORM features.
// It takes the generated Cloesce Interface Definition Language (CIDL)
// and the generated constructor registry. Both may be imported from
// "@generated/workers" as shown above.
beforeAll(() => CloesceApp.init(cidl as any, constructorRegistry));
```

Cloesce needs only the CIDL (generated interface definition) and Constructor Registry (linked Model, Service and Plain Old Object exports) to be used in tests. This means you can write tests for your Models and Services without needing to run a full Cloudflare Worker environment.

ORM methods rely on Cloudflare Workers bindings (D1, KV, R2, etc.), so you will need to mock these bindings in your test environment. The best choice for this is [Miniflare](https://developers.cloudflare.com/workers/testing/miniflare/), which provides an in memory implementation of Cloudflare Workers runtime and bindings.

A basic Miniflare setup is included in the template project which can be installed with:

```bash
npx create-cloesce my-cloesce-app
```