# Environment Declarations

Environment bindings are how Cloesce manages, references, and injects Cloudflare Workers bindings in your application.

Currently, Cloesce supports [D1](https://developers.cloudflare.com/d1/), [KV](https://developers.cloudflare.com/kv/), [R2](https://developers.cloudflare.com/r2/), [Durable Objects](https://developers.cloudflare.com/durable-objects/), and [Wrangler Environment Variables](https://developers.cloudflare.com/workers/configuration/environment-variables/).

By defining these bindings in your schema, you enable Cloesce to:

- Describe Models composed of data stored in these bindings.
- Generate a Wrangler configuration for each binding.
- Generate a [fully typed interface](./ch7-0-orm-reference.md) for interacting with each binding in your application code.
- Handle database migrations

> [!TIP]
> In this alpha, any top level declaration in Cloesce is global across any file in the project.
>
> This means that environment bindings declared in one file can be referenced and used in any other file.
