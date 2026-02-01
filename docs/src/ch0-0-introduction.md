# Introduction

> *Alpha Note*: Cloesce is under active development, expanding its feature set as it pushes towards [full
> Cloudflare support, across any language](./ch6-1-future-vision.md). In this alpha, breaking changes can occur between releases.

*The Cloesce Compiler* is a tool that enables a data first paradigm to building full stack applications with Cloudflare. Define "Models" in a high level language from which Cloesce deterministically generates and validates the required cloud infrastructure, database schemas, backend services and client code, ensuring consistency and correctness across the entire stack.

Inspired by ORMs like [Entity Framework](https://learn.microsoft.com/en-us/ef/), web frameworks that utilize Dependency Injection such as [NestJS](https://nestjs.com/) and [ASP.NET](https://dotnet.microsoft.com/en-us/apps/aspnet), interface definition and API contract tools like [Swagger Codegen](https://swagger.io/tools/swagger-codegen/) and [gRPC](https://grpc.io/), as well as Infrastructure as Code tools, Cloesce brings these concepts and much more together into a single compilation step.

<!-- langtabs-start -->
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
```python
# Coming in a later release!
```
```rs
// Coming in a later release!
```
<!-- langtabs-end -->


## Contributing

Contributions are welcome at all levels. Join our [Discord](https://discord.gg/saVTbcGHwF) to discuss ideas, report issues, or get help getting started. [Create an issue](https://github.com/bens-schreiber/cloesce/issues/new) on GitHub if you find a bug or have a feature request.

## Coalesce

Check out [Coalesce](https://coalesce.intellitect.com), an accelerated web app framework for Vue.js and Entity Framework by [IntelliTect](https://intellitect.com). Cloesce takes much of its inspiration from Coalesce (Cloesce = Cloudflare + Coalesce).