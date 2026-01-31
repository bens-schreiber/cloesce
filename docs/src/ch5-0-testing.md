# Testing

A main feature of Cloesce is the ability to easily test your Models and Services in isolation, without ever needing to mock a request or spin up a server. Unit tests that utilize Cloesce must only:
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

Cloesce needs only the CIDL and constructor registry to function in tests.

A basic miniflare setup is shown in the template project which can be installed with
```bash
$ npx create-cloesce my-cloesce-app
```