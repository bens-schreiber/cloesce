# Introduction

> *Alpha Note*: Cloesce is under active development, expanding its feature set as it pushes toward [full
> Cloudflare support across any language](./ch6-1-future-vision.md). In this alpha, breaking changes can occur between releases.

*The Cloesce Compiler* converts class definitions into a full stack Cloudflare application.

Inspired by 
- [Entity Framework](https://learn.microsoft.com/en-us/ef/)
- [NestJS](https://nestjs.com/)
- [ASP.NET](https://dotnet.microsoft.com/en-us/apps/aspnet)
- [Swagger Codegen](https://swagger.io/tools/swagger-codegen/) 
- [gRPC](https://grpc.io/)
- and Infrastructure as Code (IaC)

Cloesce is not just an ORM, migration engine, web framework, runtime validation library, IaC tool, or API Generator. It is **all of these things and more**, wrapped in a clean paradigm that makes building Cloudflare applications a breeze.

<!-- langtabs-start -->
```typescript
@Model(["GET", "SAVE", "LIST"])
class User {
    id: Integer;
    name: String;
    posts: Post[];

    @KV("user/settings/{id}", namespace)
    settings: KValue<unknown>;

    @R2("user/avatars/{id}.png", bucket)
    avatar: R2Object;

    @POST
    async hello(): User {
        // D1, KV, and R2 all hydrated here!
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

*How easy can full stack development get?*


## Contributing

Contributions are welcome at all levels. Join our [Discord](https://discord.gg/saVTbcGHwF) to discuss ideas, report issues, or get help getting started. [Create an issue](https://github.com/bens-schreiber/cloesce/issues/new) on GitHub if you find a bug or have a feature request.

## Coalesce

Check out [Coalesce](https://coalesce.intellitect.com), an accelerated web app framework for Vue.js and Entity Framework by [IntelliTect](https://intellitect.com). Many core concepts of Cloesce come directly from Coalesce (Cloesce = Cloudflare + Coalesce).