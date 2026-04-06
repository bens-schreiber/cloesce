# The Cloesce Language

> [!WARNING]
> Cloesce is under active development, expanding its feature set as it pushes toward full Cloudflare
> support across any language. The syntax and features described here are subject to change as the project evolves.

**Cloesce** is an interface definition language that describes a full stack application built on [Cloudflare's edge ecosystem](https://workers.cloudflare.com). From a simple schema, Cloesce supports features like:

| Feature              | Status |
|----------------------|--------|
| ORM                  | ✅     |
| RPC stubs            | ✅     |
| IaC                  | ✅     |
| SQL Migrations       | ✅     |
| Runtime Validation   | ✅     |
| Writing Glue         | ❌     |

## How Easy can Full Stack Development Be?

```cloesce
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

## Contributing

Contributions are welcome at all levels. Join our [Discord](https://discord.gg/saVTbcGHwF) to discuss ideas, report issues, or get help getting started. [Create an issue](https://github.com/bens-schreiber/cloesce/issues/new) on GitHub if you find a bug or have a feature request.

## Coalesce

Check out [Coalesce](https://coalesce.intellitect.com), an accelerated web app framework for Vue.js and Entity Framework by [IntelliTect](https://intellitect.com). Many core concepts of Cloesce come directly from Coalesce (Cloesce = Cloudflare + Coalesce).