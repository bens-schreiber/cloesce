# Testing

Cloesce Models and Services all live as their own isolated units with no inherent connection to an incoming Worker request, making them easy to unit test.

To write tests for Cloesce that utilize any ORM features, ensure you have:

- Have ran `npx cloesce compile` to generate the necessary files
- Run migrations for the Models being tested
- Invoke `CloesceApp.init` to initialize the Cloesce runtime

```typescript
import { CloesceApp } from "cloesce/backend";
import { cidl, constructorRegistry } from "@generated/workers";

// Cloesce must be initialized before utilizing any ORM features.
// It takes in the generated Cloesce Interface Definition Language (CIDL)
// and the generated constructor registry. Both may be imported from
// "@generated/workers" as shown above.
beforeAll(() => CloesceApp.init(cidl as any, constructorRegistry));
```

Cloesce needs only the CIDL (generated interface definition) and Constructor Registry (linked Model, Service and Plain Old Object exports) to function be used in tests.

Since Models rely on Cloudflare Workers bindings (D1, KV, R2, etc), you will need to mock these bindings in your test environment. The best choice for this is [Miniflare](https://developers.cloudflare.com/workers/testing/miniflare/), which provides an in memory implementation of Cloudflare Workers runtime and bindings.


A basic Miniflare setup is shown in the template project which can be installed with:

```bash
$ npx create-cloesce my-cloesce-app
```