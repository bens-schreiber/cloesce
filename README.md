# cloesce (alpha, v0.5.0)

> [!WARNING]
> Cloesce is under active development, expanding its feature set as it pushes toward full Cloudflare
> support across any language. The syntax and features described here are subject to change as the project evolves.

**Cloesce** is a schema language that describes a full stack application built on [Cloudflare's edge ecosystem](https://workers.cloudflare.com). From one language, generate an entire application with support for:

| Feature                 | Support |
| ----------------------- | ------- |
| ORM                     | ✅      |
| RPC stubs               | ✅      |
| Infrastructure as Code  | ✅      |
| SQL Migrations          | ✅      |
| Middleware              | ✅      |
| Runtime Type Validation | ✅      |

## How Easy can Full Stack Development Be?

```
kv Namespace {
    settings(id: int) -> json {
        "user/settings/{id}"
    }
}

r2 Bucket {
    avatar(id: int) {
        "user/avatars/{id}.png"
    }
}

d1 { Db }

[crud get, save, list]
model User for Db {
    primary {
        id: int
    }

    column {
        name: string
    }

    nav Posts::id {
        posts
    }

    kv Namespace::settings(id) {
        settings
    }

    r2 Bucket::avatar(id) {
        avatar
    }
}

api User {
    get helloWorld(self) -> User
}
```

## Documentation

See the [Cloesce Docs](https://cloesce.pages.dev) for more information on getting started, language features, architecture, and roadmap.

Utilize an LLM to interact with the docs in a conversational way:

```bash
curl https://cloesce.pages.dev/llms-full.txt -o llms-full.txt
```

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
