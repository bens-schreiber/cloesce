# cloesce (alpha, v0.1.0)

> *Alpha Note*: Cloesce is under active development, expanding its feature set as it pushes toward [full
> Cloudflare support across any language](https://cloesce.pages.dev/ch6-1-future-vision). In this alpha, breaking changes can occur between releases.

*Cloesce* converts class definitions into a full stack Cloudflare application.

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

## Documentation

See the [Cloesce Docs](https://cloesce.pages.dev) for more information on getting started, language features, architecture, and roadmap.

Utilize an LLM to interact with the docs in a conversational way:
```
curl https://cloesce.pages.dev/llms-full.txt -o llms-full.txt
```

See the [Typescript API Reference](https://cloesce-ts.pages.dev) for the generated client library documentation.

## Contributing

Contributions are welcome at all levels. Join our [Discord](https://discord.gg/saVTbcGHwF) to discuss ideas, report issues, or get help getting started. [Create an issue](https://github.com/bens-schreiber/cloesce/issues/new) on GitHub if you find a bug or have a feature request.

## Coalesce

Check out [Coalesce](https://coalesce.intellitect.com), an accelerated web app framework for Vue.js and Entity Framework by [IntelliTect](https://intellitect.com). Cloesce takes much of its inspiration from Coalesce (Cloesce = Cloudflare + Coalesce).)

# Building, Formatting, Testing

All relevant commands can be found in the `Makefile` in the project root. Run `make all` to build, format and test all packages.