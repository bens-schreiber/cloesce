# Proposal: The Cloesce Language

> [!NOTE]
> The Cloesce Language was developed before the proposal, and thus this is written retroactively to document the design decisions and rationale behind the language.

- **Author(s):** Ben Schreiber
- **Status:** Draft | Review | Accepted | Rejected | **Implemented**
- **Created:** 2026-04-06
- **Last Updated:** 2026-04-06

---

## Summary

This proposal introduces the Cloesce Language, a domain-specific language designed to define data models and their relationships in a way that is lightweight and readable. The Cloesce Language serves as the new foundation for the Cloesce framework, enabling developers to easily define their data structures and generate the necessary code for database interactions and API endpoints.

---

## Motivation

The Cloesce Language was created to address the need for a more intuitive and efficient way to define data models within the Cloesce framework. Prior to the introduction of the Cloesce Language, Cloesce relied entirely on a process of parsing existing ASTs from TypeScript code to generate the CIDL representation of the data models. This approach, while functional, had several drawbacks:

1. **Verbosity**: Defining data models in TypeScript was an eyesore, especially for larger projects with complex relationships.
2. **ORM Lacks Type Safety**: The ORM layer accepted generic unknown parameters which were not type safe, leading to potential runtime errors and a poor developer experience.
3. **Limited Expressiveness**: The TypeScript-based approach made it difficult to express certain concepts such as composite foreign keys, uniqueness constraints, and other database-specific features in a clear and concise manner.
4. **Performance**: Parsing TypeScript code to generate the CIDL representation added unnecessary overhead to the compilation process.
5. **Boilerplate required to extend the language**: Adding a new language to Cloesce required writing a new parser, which is a non-trivial amount of work and adds maintenance overhead.
6. **Lack of Separation of Concerns**: Mixing the definition of data models with application logic in TypeScript files made it harder to maintain a clear separation of concerns, leading to less organized codebases.

... and many more. With a dedicated language for Cloesce, we can create a simple, extendable syntax that is designed specifically for defining data models and their relationships, while also providing a more enjoyable developer experience.

---

## Goals and Non-Goals

### Goals

- Create a domain-specific language for defining data models in Cloesce.
- Ensure the language is lightweight and easy to read, as developers will not be familiar with it.
- Provide a clear mapping from the Cloesce Language to the underlying CIDL representation used by the Cloesce framework.
- Enable the generation of type-safe code for database interactions and API endpoints based on the defined data models.
- Allow for extensibility in the language to accommodate future features and concepts that may arise in the Cloesce framework.

### Non-Goals

- The Cloesce Language is not intended to be a general-purpose programming language and should not include features unrelated to defining data models.
- The language is not designed to replace TypeScript for application logic, but rather to complement it by providing a dedicated syntax for data model definitions.
- The initial implementation of the Cloesce Language will focus on core features necessary for defining data models and their relationships, with more advanced features (LSP, formatter, SQL semantic analysis, etc.) to be considered for future iterations.

---

## Detailed Design

The Cloesce Language syntax is inspired largely by Rust, but is unique in it's own right. In this section we will go over the compilation process of the Cloesce Language, how it maps to CIDL, and the rationale behind some of the design decisions.

### Frontend Architecture

Cloesce currently uses Rust for it's compiler, and we will continue to use Rust for the Cloesce Language. The frontend (lexing, parsing, semantic analysis) is implemented using the `logos` lexing crate and the `chumsky` parsing crate.

The frontend takes in a `.clo` or `.cloesce` file containing the Cloesce Language definitions and follows a three step process:

1. **Lexing**: The input file(s) are tokenized into a stream of tokens using the `logos` crate. This involves defining a set of token types (e.g. identifiers, keywords, symbols) and the rules for how to recognize them in the input text.
2. **Parsing**: The stream of tokens is then parsed an IR called the Parse AST using the `chumsky` crate. This involves defining a grammar for the Cloesce Language and how the tokens can be combined to form valid constructs (e.g. models, fields, relationships).
3. **Semantic Analysis**: The Parse AST is then transformed into the Cloesce AST (CIDL) which is the internal representation used by the Cloesce compiler. This step involves performing various checks and transformations to ensure the validity of the defined models and their relationships, as well as expanding the constructs in the Cloesce Language to the corresponding constructs in CIDL.

Unlike many other compilers, Cloesce will have no dedicated client-binary to call for the frontend (at least for the sake of this proposal), instead relying on a `NodeJS` package that will call WASI functions exported by the Rust compiler. The rationale behind this decision is twofold:

1. **Simplicity**: Developers do not have to worry about installing and managing a binary for the Cloesce compiler that is compatible with their system.
2. **Integration**: Since the config for Cloesce is implemented in TypeScript, it is easier to draw in environment variables and do custom dynamic configuration during compilation if the frontend is exposed as a package that can be imported and called directly from the config file.

When more languages are added to Cloesce in the future, they will likely follow this same process (e.g. a Python `pip` package that calls the same WASI functions), allowing us to maintain a single codebase for the frontend while providing multiple language options for developers to use in their config files.

### Error Handling

Each stage of the frontend can produce errors (lex error, parse error, semantic error). All errors are marked up with the `ariadne` crate, which provides a pretty error display with spans, suggestions and notes displayed to the console.

### Language Design

> [!NOTE]
> Code snippets in this section do not have Cloesce syntax highlighting, as we are constrained by markdown and GitHub.
> Rust highlighting is used to get some color variation.

Cloesce was decided to be a global language spanning multiple files, meaning that all symbols within it are accessible across all files within a Cloesce project. This decision was made to simplify the mental model for developers, as they do not have to worry about imports and exports between files when defining their data models. Instead, they can simply define their models in any file and reference them from anywhere else in the project. This is open to change in the future.

#### Defining a Wrangler Environment

Cloesce uses a reserved keyword `env` to define the Cloudflare Wrangler environment, containing all D1, KV, R2, and variable bindings.

```rs
env {
    d1 {
        binding1
        binding2
    }

    kv {
        binding1
        binding2
    }

    r2 {
        binding1
        binding2
    }

    var {
        var1: string
        var2: int
    }
}
```

The above snippet will translate directly to a `wrangler.toml` or `wrangler.jsonc`, like the old `@WranglerEnv` decorator.

### Defining a Model

Models are defined using the `model` keyword, followed by the name of the model and a block containing the field definitions. Each field is defined with a name and type.

A model that has a D1 table associated with it must use the `[use]` tag, linking it to a binding in the `env` block.
The `[use]` tag can also specify CRUD operations.

```rs
[use d1]
[use get, save, list]
model User {
    primary {
        id: int
        id2: string
    }

    name: string
}
```

A primary key must be composed within a `primary` block, and can be made up of several fields. Any number of `primary` blocks can be specified, all relating to one single primary key.

### Unique Constraints

The `unique` block encompasses any fields that are to be apart of a unique constraint:

```rs
model User {
    primary {
        id: int
    }

    unique {
        email: string
        name: string
    }
}
```

A primary block cannot be composed by a unique block, or vice versa.

### Foreign Keys

Foreign keys are defined by using a `foreign` block which takes in another Model's field as a parameter, using a well known `::` syntax:

```rs
model User {
    primary {
        id: int
    }

    foreign (Post::id) {
        postId
    }
}
```

The above snippet defines a `User` model with a foreign key `postId`. The type will automatically resolve to the type of the `id` field in the `Post` model.

Foreign keys can be composite, and also marked with an `optional` key to indicate that the relationship is nullable:

```rs
model User {
    primary {
        id: int
    }

    foreign (Post::id1, Post::id2) optional {
        postId
    }
}
```

Foreign keys can also be unique:

```rs
model User {
    primary {
        id: int
    }

    unique foreign (Post::id) {
        postId
    }

    // or
    unique {
        foreign (Post::id) {
            postId
        }
    }
}
```

### Navigation Fields

Navigation fields can specify 1:1, 1:M or M:M relationships between models.

A 1:1 relationship is nested within a foreign key:

```rs
model User {
    // ...
    foreign (Post::id) {
        postId

        nav { post }
    }
}
```

A 1:M relationship is defined by a `nav` block that specifies the related model and the field in that model that relates back to the current model:

```rs
model User {
    // ...

    nav (Post::authorId) {
        posts
    }
}
```

An M:M relationship is defined by a `nav` block that specifies the related model and the fields in both models that relate to each other:

```rs
model User {
    // ...

    nav (Post::id) {
        posts
    }
}

model Post {
    // ...

    nav (User::id) {
        users
    }
}
```

### Key Fields

A key field is a non-stored parameter passed during API invocations.

```rs
model User {
    keyfield {
        someKey
    }
}
```

They require no type and resolve to a string at runtime.

### Defining an API

API endpoints are used by creating an `api` block for a model. Any number of `api` blocks can specify any number of endpoints, which will be generated under one API namespace for that model.

The `self` key word is used to make a method `instantiated`, and the lack of it makes the method static.

```rs
model User { }

api User {
    get getUser(id: int) -> User
    post doNothing(self) -> void
    delete deleteUser(self) -> void
}

// valid to keep defining blocks
api User {
    get getUserByEmail(email: string) -> User
}
```

### Defining a Data Source

Data Sources are defined using the `source` keyword followed by some unique under a models namespace. An `include` block specifies the include tree, and optional `get` / `list` blocks specify the data source methods

```rs
source WithPosts for User {
    include {
        posts
    }

    sql get(id: int) {
        "
        SELECT * FROM ($include)
        WHERE id = $id
        "
    }

    sql list(lastId: int, limit: int) {
        "
        SELECT * FROM ($include)
        WHERE id > $lastId
        ORDER BY id ASC
        LIMIT $limit
        "
    }
}
```

All SQL methods can take in parameters referenced with a `$` sigil, and the `$include` parameter is reserved for the include tree. These are translated to positional arguments `?1, ?2, ...` in the generated SQL, so as to avoid SQL injection vulnerabilities.

Additionally, an API method can specify a data source with the `[source]` tag on `self`:

```rs
model User { }

api User {
    get getUserWithPosts([source WithPosts] self, id: int) -> UserWithPosts // uses the get() method of the WithPosts data source
}
```

### Defining a Service and Injectables

Services have been modifed to allow any field type within them, and are defined using the `service` keyword.

An `inject` block can also specify any globally injectable value

```rs
inject {
    GmailApi
}

service EmailService {
    api: GmailApi
}

api EmailService {
    get sendEmail(self, to: string, subject: string, body: string) -> void
}
```

### Plain Old Objects

The `poo` keyword defines a plain old object, which is simply a namespace for some fields. This is useful for defining return types for API methods that do not correspond to a Model.

```rs
poo UserWithPosts {
    id: int
    name: string
    posts: array<Post>
}
```

### Primitives and Generics

The Cloesce Language supports the following type primitives:

- `int`
- `string`
- `bool`
- `date`
- `double`
- `blob`
- `json`
- `stream`
- `R2Object`
- `array<T>`
- `Option<T>`
- `Paginated<T>`
- `Partial<T>`
- `KvObject<T>`
- `DataSource<T>`

Where `T` can be any type, including user defined models and poos.
