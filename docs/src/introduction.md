# Introduction

> *Note*: Cloesce is under active development, expanding its feature set as it pushes towards full
> Cloudflare support, across any language. In this alpha, breaking changes can occur between releases.

*The Cloesce Compiler* enables a data-first approach to building full-stack applications on Cloudflare. Developers define their data models and relationships once, in a high level language, and Cloesce deterministically generates the required cloud infrastructure, backend services, and client code, ensuring consistency and correctness across the entire stack.

Inspired by modern ORMs like [Entity Framework](https://learn.microsoft.com/en-us/ef/), web frameworks that utilize Dependency Injection such as [NestJS](https://nestjs.com/) and [ASP.NET](https://dotnet.microsoft.com/en-us/apps/aspnet),  interface definition and API contract tools like [Swagger Codegen](https://swagger.io/tools/swagger-codegen/) and [gRPC](https://grpc.io/), and many Infrastructure as Code frameworks, Cloesce brings these concepts together into a single deterministic compilation step.

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

Cloesce is very much a work in progress. We welcome contributions on all levels, from documentation improvements to new features, bug fixes and suggestions. Join our [Discord](https://discord.gg/saVTbcGHwF) to discuss ideas, report issues, or get help getting started.

## Coalesce

Check out [Coalesce](https://coalesce.intellitect.com), an accelerated web app framework by [IntelliTect](https://intellitect.com) which Cloesce takes much of its inspiration from (Cloesce = Cloudflare + Coalesce).