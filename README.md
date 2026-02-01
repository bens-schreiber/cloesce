# cloesce (alpha, v0.1.0)

*The Cloesce Compiler* is a tool that enables a data first paradigm to building full stack applications with Cloudflare. Define "Models" in a high level language from which Cloesce deterministically generates and validates the required cloud infrastructure, database schemas, backend services and client code, ensuring consistency and correctness across the entire stack.

Inspired by ORMs like [Entity Framework](https://learn.microsoft.com/en-us/ef/), web frameworks that utilize Dependency Injection such as [NestJS](https://nestjs.com/) and [ASP.NET](https://dotnet.microsoft.com/en-us/apps/aspnet), interface definition and API contract tools like [Swagger Codegen](https://swagger.io/tools/swagger-codegen/) and [gRPC](https://grpc.io/), as well as Infrastructure as Code tools, Cloesce brings these concepts and much more together into a single compilation step.

```typescript
@Model(["GET", "SAVE", "LIST"])
class User {
    id: Integer;
    name: String;

    @OneToMany<Post>(p => p.userId)
    posts: Post[];

    @KV("user/settings/{id}", namespace)
    settings: KValue<unknown>;

    @R2("user/avatars/{id}.png", bucket)
    avatar: R2Object;

    @POST
    async hello(): User {
        // Everything is hydrated here! Magic!
        return this;
    }
}
```

## Documentation

See the [Cloesce Compiler Docs](https://cloesce.pages.dev/) for full documentation and quick start guides.

See the [TypeScript API Docs](https://cloesce-ts.pages.dev/) for generated API reference documentation.


## Contributing

Contributions are welcome at all levels. Join our [Discord](https://discord.gg/saVTbcGHwF) to discuss ideas, report issues, or get help getting started. [Create an issue](https://github.com/bens-schreiber/cloesce/issues/new) on GitHub if you find a bug or have a feature request.

## Coalesce

Check out [Coalesce](https://coalesce.intellitect.com), an accelerated web app framework for Vue.js and Entity Framework by [IntelliTect](https://intellitect.com). Cloesce takes much of its inspiration from Coalesce (Cloesce = Cloudflare + Coalesce).


# Testing the Compiler

## `make all`

- In the root folder, run `make all` to format, build and test all components.

## Unit Tests

- `src/ts` run `npm test`
- `src/generator` run `cargo test`

## Integration Tests

- Regression tests: `cargo run --bin regression`

Optionally, pass `--check` if new snapshots should not be created.

To target a specific fixture, pass `--fixture folder_name`

To update integration snapshots, run:

- `cargo run --bin update`

To delete any generated snapshots run:

- `cargo run --bin update -- -d`

## E2E

- `tests/e2e` run `npm test`

## Code Formatting

- `cargo fmt`, `cargo clippy`, `npm run format:fix`
