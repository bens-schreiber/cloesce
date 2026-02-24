# Proposal: Data Sources

- **Author(s):** Ben Schreiber
- **Status:** Draft | Review | Accepted | Rejected | **Implemented**
- **Created:** 2026-02-19
- **Last Updated:** 2026-02-21

---

## Summary

Data Sources are instructions on how to fetch and hydrate the relationships of a Cloesce Model. They are composed of an `Include Tree`, and an optional SQL statement. This proposal aims to differentiate Data Sources and Include Trees, and move the control of Data Source selection from the client to the server, allowing instantiated Model methods to specify exactly how they should be hydrated.

Additionally, this proposal implements a "default" Data Source for each Model, which includes all near relationships, and only the shallow side of far relationships, giving a more intuitive default for how Models should be hydrated.

---

## Motivation

Data Sources currently exist in the `v0.1.0` alpha build of Cloesce, but have no real distinction from the `Include Tree`. In fact, they are completely left out of the documentation for the alpha, with only a section on Include Trees existing.

The current half-implementation of Data Sources has led to confusion and significant boilerplate for developers trying to use them. They also lack the ability to work with complex SQL filtering and ordering, which is a major use case for them in [frameworks like Coalesce](https://coalesce.intellitect.com/modeling/model-components/data-sources.html).

To illustrate the problems with the current implementation, consider this Model:

```ts
@Model(["LIST"])
class User {
    id: Integer;
    name: String;

    posts: Post[];

    @R2("{id}", bucket)
    object: R2ObjectBody | undefined;

    @KV("{id}", namespace)
    metadata: KValue<unknown> | undefined;
}
```

### Problem a) Unnecessary Delegation to Client

The `User` Model has no Data Sources defined. The current implementation always provides a "none" Data Source to the client, which does not fetch any relationships. Thus, when a client calls `User.LIST()`, they will not receive the `posts`, `object`, or `metadata` fields of the `User`, as they are not included in the "none" Data Source.

To fix this, the current solution is to create a Data Source that includes all of the relationships on the Model, and instruct the client to use that Data Source when they want to fetch all of the data for a `User`:

```ts
class User {
    // ...

    static readonly includeAll: IncludeTree<User> = {
        posts: {},
        object: {},
        metadata: {}
    }
}

User.LIST("includeAll");    // yields all data
User.LIST("none");          // yields no data
User.LIST();                // yields no data, as the default Data Source is "none"
```

For CRUD methods like `LIST`, `GET`, and `SAVE`, this is the intended functionality — generic generated methods should let the client decide what they want to fetch.

However, for custom instance methods, this doesn't make any sense. For example, consider a method that allows a client to download the `object` field of a `User` as a stream:

```ts
@Model(["LIST"])
class User {
    id: Integer;
    name: String;

    posts: Post[];

    @R2("{id}", bucket)
    object: R2ObjectBody | undefined;

    @KV("{id}", namespace)
    metadata: KValue<unknown> | undefined;

    @GET
    downloadObject(): HttpResult<ReadableStream> {
        if (!this.object) {
            return HttpResult.fail(404, "Object not found");
        }
        return HttpResult.ok(200, this.object.body);
    }
}
```

First, the method `downloadObject` will return a 404 if the client does not use the `"includeAll"` Data Source. We've essentially enabled the client to make a mistake that causes a bad response, and adds no functionality — the client has no reason to call `downloadObject` with the `"none"` Data Source, and if they do, it only results in a 404.

Second, the `"includeAll"` Data Source fetches `posts` and `metadata`, which are not needed for this method. This is inefficient, as only the `object` field is necessary. To fix this, a developer might create another Data Source specifically to fetch the `object` field only:

```ts
@Model()
class User {
    // ...

    static readonly onlyObject: IncludeTree<User> = {
        object: {}
    }
}
```

Although the above solution works, it adds boilerplate, and again gives the client an option to do something that benefits no one — why should the client even have the option to fetch `posts` and `metadata` when calling `downloadObject`? Why should they have the option to call this method with `"none"`?

### Problem b) Unable to Filter Data

The current `Include Tree`-only approach for fetching data is powerful, but does not allow even basic SQL filtering and ordering.

For example, if a client wants to fetch all `User`s with their `posts`, alphabetically ordered by `name`, and only wants names starting with "A–J", they would need to create a static method to do so:

```ts
@Model(["LIST"])
class User {
    // ...
    
    @GET
    static async listAlphabetically(@Inject env: Env): Promise<User[]> {
        const db = env.db;
        const joined = Orm.select(User, {
            includeTree: { posts: {} },
            from: "SELECT * FROM User WHERE name >= 'A' AND name <= 'J' ORDER BY name ASC"
        });

        const result = await db.prepare(joined);
        const mapped = Orm.map(User, result.results, { posts: {}});
        return mapped;
    }
}
```

This is a lot of boilerplate for a simple query. Furthermore, if several methods required similar filtering and ordering, the logic for `listAlphabetically` would need to be repeated in each method, or abstracted so that it can be used in multiple methods (e.g. accept different Include Trees, run `hydrate` if necessary).

### Problem c) Include Nothing By Default

The current default Data Source of "none" is not a good default for most use cases. It stands to reason that if relationships are defined on a Model, they are likely useful to the functionality of the Model. To include anything less than all relationships is a specific decision from the developer, such as creating a specific method that only utilizes some parts of the Model.

---

## Goals and Non-Goals

### Goals
- Separate Data Sources from the Include Tree, giving them a distinct purpose
- Move control of Data Sources from the client to each Model method
- Allow Data Sources to specify SQL Select statements, giving more control over filtering and ordering
- Create a "default" Data Source that populates the Include Tree with near relationships

### Non-Goals
- Validation of SQL in Data Sources

> *Note*: Validation of Data Source SQL could be a future feature. The ORM map function will fail if the result is malformed in some way, and this could be caught at compile time with some clever analysis of the statement, but this is not a priority for the initial implementation of Data Sources, and is not necessary to achieve the goals of this proposal.

---

## Detailed Design

### Data Source Interface

This proposal introduces a new `DataSource` interface to the frontend. It will replace the `DataSourceOf<T>` type that is interpreted by the compiler as the `DataSource` CIDL type. The new interface has two properties: `includeTree` and `select`.

```ts
interface DataSource<T> {
    includeTree?: IncludeTree<T>;
    select?: (joined: (from?: string) => string) => string;
}
```

- `includeTree` is used to determine what relationships should be fetched, and is used by the compiler to determine what SQL joins should be made.

- `select` is a function that accepts a `joined` function (literally `Orm.select` using the Data Source's `includeTree`), and returns a SQL Select statement. This allows the Data Source to specify complex filtering and ordering logic, while still relying on the compiler to determine the necessary joins based on the `includeTree`.

> *Note*: If `includeTree` is not defined, the `joined` function in `select` will be called with an empty `includeTree`. This has some use cases, such as when a developer wants to extend the default Data Source functionality (see the next section for an example), but for the most part, Data Sources should have an `includeTree` defined.

> *Note*: It is possible to have a malformed SQL statement in `select` which will cause a runtime error. Additionally, a query that returns data in an order that `Orm.map` cannot handle may fail silently or throw an error. Validation of the SQL statement is out of scope for this proposal, but could be a future feature.

### Default Data Source

During the generator step of compilation, Cloesce will search each Model for a Data Source called "default". If one does not already exist, Cloesce will create a default which includes all near relationships of the Model, meaning any KV, R2, one-to-one relationships, and only the shallow side of one-to-many and many-to-many relationships.

This allows for a more intuitive default for how Models should be hydrated, while still giving developers the option to create more specific Data Sources for specific use cases.

For example, given this Model:
```ts
// ... basic Toy and Post models

@Model()
class Dog {
    // ...
    user: User;
    toys: Toy[];
}

@Model()
class User {
    // ...
    dog: Dog;

    posts: Post[];

    user: User | null;

    @R2("{id}", bucket)
    object: R2ObjectBody | undefined;

    @KV("{id}", namespace)
    metadata: KValue<unknown> | undefined;
}
```

the default Data Source for `User` would be:
```ts
const default: DataSource<User> = {
    includeTree: {
        dog: {
            toys: {
                // No further relationships are included
            }
        },
        posts: {
            // No further relationships are included
        },
        user: {
            // No further relationships are included
        },
        object: {},
        metadata: {}
    }
}
```

Since the default Data Source is generated by the compiler, it will not be accessible to the backend. To address this, an ORM method will be provided to run the same logic as the compiler to generate the default Data Source for a Model at runtime:

```ts
const defaultDataSource = Orm.defaultDataSource(User);
```

### Defining a Public Data Source

A Model can define any number of public Data Sources by creating `static readonly` properties typed to the `DataSource<this>` interface. Public Data Sources will be shared with the client such that the client can pass them as parameters or receive them as return values from Model methods.

For example:

```ts
@Model()
class User {
    // ...

    static readonly includeAll: DataSource<User> = {
        includeTree: {
            dog: {
                toys: {}
            },
            posts: {},
            object: {},
            metadata: {}
        }
    }

    static readonly onlyNameAndPosts: DataSource<User> = {
        includeTree: {
            posts: {}
        },
        select: (joined) => `${joined()} WHERE name IS NOT NULL`
    }

    @GET
    acceptAndReturnDataSource(ds: DataSource<User>): DataSource<User> {
        return ds;
    }
}
```

The client can now pass `User.includeAll` or `User.onlyNameAndPosts` into any Model method that accepts a `DataSource<User>`, and can also receive them as return values from any Model method that returns a `DataSource<User>`.

### Using Data Sources in Model Methods

By default, all instance methods of a Model will hydrate using the "default" Data Source. Static methods will not pass semantic analysis if they have a data source defined. A method can specify a different Data Source by passing it explicitly to the HTTP verb decorator:

```ts
@Model()
class User {
    // ...

    static readonly includeAll: DataSource<User> = {
        includeTree: {
            dog: {},
            posts: {}
        }
    }

    @GET()
    foo() {
        // hydrated with the same DS as Orm.defaultDataSource(User)
    }

    @GET(User.includeAll)
    bar() {
        // hydrated with User.includeAll
    }
}
```

This purposefully removes the behavior of a client specifying the Data Source when calling an instantiated method. The old behavior is still possible via a static method that accepts a `DataSource` as a parameter (which the CRUD methods already do).

### Defining an Inline Data Source

A method can also define a Data Source inline, or via a constant defined outside of the Model. This is useful for methods that require a specific Data Source that the client does not need to be aware of.

```ts
const privateDataSource: DataSource<User> = {
    includeTree: {
        posts: {}
    },
    select: (joined) => `${joined()} WHERE name IS NOT NULL`
}

@Model()
class User {
    // ...

    @GET(privateDataSource)
    foo() {
        // hydrated with privateDataSource
    }

    @GET({ posts: {} }, joined => `${joined()} WHERE name IS NOT NULL`)
    bar() {
        // hydrated with the same DS as privateDataSource, but defined inline on the decorator
    }
}
```

Neither of these Data Sources will be available on the client.


### ORM Methods

All ORM methods that previously accepted an `IncludeTree` will now accept a `DataSource`. For example:

```ts
const users = await Orm.select(User, {
    dataSource: User.onlyNameAndPosts
});
```

These methods will respect both the `includeTree` and `select` properties of the Data Source, meaning they will perform the necessary joins based on the `includeTree`, and use the SQL statement returned by `select` if one exists.

They will default to the "default" Data Source if no Data Source is provided.


### Implementation Details

This proposal will require changes to the compiler, frontend, and runtime.

#### Compiler

#### Extending the Data Source Attribute of `Model`

The compiler already has the `DataSource` type in its grammar, and allows every Model to have a list of `DataSource`s. However, these Data Sources are composed of only an `IncludeTree`.

To implement the new interface, a `private` flag will be added to the `DataSource` struct in the AST. Any inline or private Data Source will be marked as `private`, while any Data Source defined as a static property on the Model will be marked as `public`.

#### Default Data Source Generation

Each `Model` will be extended to have an optional `default_data_source` property. During the Workers generation step of compilation, the compiler will check if this value exists, and if not, generate a default Data Source by traversing the Model's fields and including all near relationships, and only the shallow side of far relationships.

#### Client Generation

The client will no longer accept a Data Source on every instance method.

#### Frontend

The frontend will need to be updated with the new Data Source interface.

All HTTP verb decorators will be updated to take an optional Data Source parameter, be it inline or a static property on the Model.

#### Runtime

#### Private Data Source Singleton

To support inline and private Data Sources, a "private data source repository" singleton will be added to the runtime. This will be populated by the updated HTTP verb decorators, which take their Data Source parameter and add it to the repository.

The repository will be a simple key-value store, where the key is the hash of the Data Source value, method name, and Model name. When an instance method is called, the runtime will check if it has a private Data Source in the AST, and if it does, it will look up the Data Source in the repository using that hash, and use it for hydration.

This complexity is necessary because the `select` statement of a Data Source cannot be serialized into the CIDL (it is not a constant value), but we still want to support inline Data Sources defined in the decorators. By moving the Data Source into a singleton at runtime, we can support this use case without needing to serialize the `select` function into the CIDL.

#### ORM Method (WASM)

The WASM internals of the runtime will remain exactly the same. They only need Include Trees to determine a SQL output.

The `select` function of the Data Source takes a `join` function that is a wrapper around the `Orm.select` function, with the Data Source's `includeTree` already applied. This is handled in the TypeScript runtime.

#### Cloesce Router

The Cloesce Router will be updated to no longer require a Data Source to be passed for instantiated methods, because the Data Source will now be static to the method itself. The router will check if the method being called has a Data Source in its AST, and if so, will use that Data Source for hydration. If it does not, it will default to the "default" Data Source for that Model.

---

## Implementation Plan

This proposal will be implemented in a single pull request for the v0.2.0 release, as it is a breaking change.