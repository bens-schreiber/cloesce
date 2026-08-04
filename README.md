# cloesce (alpha, v0.6.0)

> [!WARNING]
> Cloesce is under active development, expanding its feature set as it pushes toward full Cloudflare
> support across any language. The syntax and features described here are subject to change as the project evolves.

**Cloesce** is a schema language that describes a full stack application built on [Cloudflare's edge ecosystem](https://workers.cloudflare.com).

| Feature                 | Support |
| ----------------------- | ------- |
| ORM                     | ✅      |
| Query Planner           | ✅      |
| RPC                     | ✅      |
| SQL Migrations          | ✅      |
| Runtime Type Validation | ✅      |
| Infrastructure as Code  | 🟨      |

## Overview

Cloesce compiles `.clo` schema files into a typed ORM, an RPC API, SQL migrations, and generated client/backend
code, all wired to Cloudflare bindings (D1, Durable Objects, KV, R2).

**1. Declare environment bindings.** Each binding maps to a Cloudflare resource and can be injected into APIs:

```
// D1 database
d1 {
    SubRedditDb
}

// KV namespace
kv Sessions {
    session -> string {
        token: string
    }
}

// R2 bucket
r2 Avatar {
    avatar {
        name: string
    }
}

// Durable Object
durable UserDo {
    shard {
        name: string
    }
}

// Custom auth context, injectable into any API
inject {
    AuthUser
}
```

**2. Model your data, composing across storage backends.** 1:1 (`one`), 1:M (`many`), KV-backed fields, and
DO-sharded models all compose together:

```
// A SQL table in D1
model SubReddit for SubRedditDb {
    primary {
        id: int
    }

    column {
        title: string
        description: string
    }

    // 1:M
    many SubRedditPost::subRedditId(id) {
        posts
    }
}

// Sharded by doId, one Durable Object per Post
model Post for PostDo::doId {
    // KV-backed field
    kv PostDo::{ meta, doId(doId) } {
        meta
    }

    // 1:M, sharded by DO
    many Comment::doId {
        comments
    }
}

model Comment for PostDo::doId {
    primary {
        id: int
    }

    column {
        upvotes: int
        authorName: string
        content: string
    }

    // 1:1, resolved across a different Durable Object
    one User::name(authorName) {
        author
    }
}
```

**3. Define APIs and call the generated backend, with env injection and one-shot hydration:**

```
api Post {
    post create -> Post {
        subRedditId: int
        title: string
        content: string

        inject { SubRedditDb PostDo AuthUser UserDo }
    }
}
```

```ts
export const post = {
  async create(env, subRedditId, title, content) {
    const doId = crypto.randomUUID();
    const meta = { title, content, upvotes: 0 };

    await env.PostDo.Post.save(doId, { doId, meta });
    await env.SubRedditDb.SubReddit.save({
      id: subRedditId,
      posts: [{ postId: doId, subRedditId }],
    });

    // One call, fully hydrated.
    return env.PostDo.Post.hydrateAll([{ doId }], "Post");
  },
} satisfies clo.Api.Post.Of;
```

**4. Call it from the generated, fully-typed client:**

```ts
import { Post, SubReddit } from "@cloesce/client.js";

// GET a hydrated SubReddit.
const sub = await SubReddit.$get(subRedditId);

// Create a new post.
const post = await Post.create(subRedditId, "title", "body", fetch);

// Instance methods are available on hydrated results.
await post.data!.vote(1, fetch);
```

**5. Cloesce diffs your schema into `wrangler.jsonc`** (bindings, migrations) so infrastructure stays in sync:

```jsonc
{
  "d1_databases": [
    {
      "binding": "SubRedditDb",
      "migrations_dir": "./migrations/SubRedditDb"
    }
  ],
  "durable_objects": {
    "bindings": [{ "class_name": "PostDo", "name": "PostDo" }]
  },
  "kv_namespaces": [{ "binding": "Sessions", "id": "replace_with_Sessions_id" }],
  "r2_buckets": [
    {
      "binding": "Avatar",
      "bucket_name": "replace-with-avatar-name"
    }
  ]
}
```

See the [Examples](https://github.com/bens-schreiber/cloesce/tree/main/examples) folder for more examples of Cloesce in action.

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
