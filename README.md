# cloesce (alpha, v0.3.0)

> [!WARNING]
> Cloesce is under active development, expanding its feature set as it pushes toward full Cloudflare
> support across any language. The syntax and features described here are subject to change as the project evolves.

**Cloesce** is a schema language (or IDL) that describes a full stack application built on [Cloudflare's edge ecosystem](https://workers.cloudflare.com). It provides a single source of truth for your application, with a single language to define:

| Feature            | Status |
| ------------------ | ------ |
| D1,KV,R2 ORM       | ✅     |
| RPC stubs          | ✅     |
| Middleware         | ✅     |
| IaC                | ✅     |
| SQL Migrations     | ✅     |
| Runtime Validation | ✅     |

## How Easy can Full Stack Development Be?

```
env {
    d1 { db }
    kv { namespace }
    r2 { bucket }
}

[use db]
[use get, save, list]
model User {
    primary {
        id: int
    }

    nav(Posts::id) {
        posts
    }

    kv(namespace, "user/settings/{id}") {
        settings: json
    }

    r2(bucket, "user/avatars/{id}.png") {
        avatar
    }

    name: string
}

api User {
    get helloWorld(self) -> User
}
```

## Documentation

See the [Cloesce Docs](https://cloesce.pages.dev) for more information on getting started, language features, architecture, and roadmap.

Utilize an LLM to interact with the docs in a conversational way:

```
curl https://cloesce.pages.dev/llms-full.txt -o llms-full.txt
```

See the [Typescript API Reference](https://cloesce-ts.pages.dev) for the generated client library documentation.

## VS Code Extension

A basic language highlighting extension for Cloesce is available in the [VS Code marketplace](https://marketplace.visualstudio.com/items?itemName=BenSchreiber.cloesce-lang). In the future, this extension will also include a full LSP server.

More editor integrations are planned for the future (and you can always contribute your own!). If you're interested in contributing an editor extension, reach out in the [Discord](https://discord.gg/saVTbcGHwF) server.

## Contributing

Contributions are welcome at all levels. Join our [Discord](https://discord.gg/saVTbcGHwF) to discuss ideas, report issues, or get help getting started. [Create an issue](https://github.com/bens-schreiber/cloesce/issues/new) on GitHub if you find a bug or have a feature request.

## Coalesce

Check out [Coalesce](https://coalesce.intellitect.com), an accelerated web app framework for Vue.js and Entity Framework by [IntelliTect](https://intellitect.com). Cloesce takes much of its inspiration from Coalesce (Cloesce = Cloudflare + Coalesce).

# Building, Formatting, Testing

## Prerequisites

Before building, ensure you have the required dependencies installed:

**Required:**

- [Rust](https://rustup.rs/) (with `wasm32-unknown-unknown` target)
- [Node.js](https://nodejs.org/)
- [Pnpm](https://pnpm.io/installation)

**Optional:**

- [pandoc](https://pandoc.org/) (for documentation) - `brew install pandoc`
- [mdbook](https://rust-lang.github.io/mdBook/) (for documentation) - `cargo install mdbook`

Run `make check-deps` to verify your setup.

## Build Commands

All relevant commands can be found in the `Makefile` in the project root. Run `make all` to build, format and test all packages.
