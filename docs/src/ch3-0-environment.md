# Environment Declaration

Environment bindings are how Cloesce, manages, references and injects Cloudflare Workers bindings in your application.

Currently, Cloesce supports [D1](https://developers.cloudflare.com/d1/), [KV](https://developers.cloudflare.com/kv/), [R2](https://developers.cloudflare.com/r2/), [Durable Objects](https://developers.cloudflare.com/durable-objects/), and [Wrangler Environment Variables](https://developers.cloudflare.com/workers/configuration/environment-variables/).

> [!TIP]
> Any top level declaration in Cloesce is global across any file in the project. This means that environment bindings declared in one file can be referenced and used in any other file.

> [!TIP]
> Environment bindings can be [injected](./ch6-3-dependency-injection.md) to any API implementation:
>
> ```cloesce
> api MyApi {
>   inject { Db }
> }
> ```
