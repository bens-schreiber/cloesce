# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-04-26
- **Last Updated:** 2026-04-26

---

## Summary

[Durable Objects](https://developers.cloudflare.com/durable-objects/) are a Cloudflare Workers primitive that provide a way to model "stateful objects" in a distributed environment. Importantly, they enable a database-per-object model, and are capable of maintaining web socket connections. This proposal details a first class integration of Durable Objects into Cloesce.

---

## Motivation

Durable Objects are easily the most powerful and unique feature of Cloudflare Workers. Because Cloudflare has no true "monolithic" relational database (each D1 database is capped at 10GB), Durable Objects are the best solution for stateful applications that rely on relational data. In fact, most Cloudflare products are built on top of Durable Objects. With a solution to model a system of Durable Objects, Cloesce can be used in a wide variety of applications, including those that require real-time communication via web sockets.

---

## Goals and Non-Goals

### Goals
- Define Durable Objects in the `env` schema declaration.
- Enable a Model to be backed by a Durable Object (instead of a D1 database).
- Enable a Model to use a DO based KV store for KV fields (instead of a KV bucket).
- Enable a Service to be backed by a Durable Object (instead of a Worker).
- Generate migrations for any SQL schema changes in a DO Model.
- First class web socket APIs (including backend and client generation).
- Wrangler configuration for migrating and deploying DOs.

### Non-Goals
- Support for D1 backed models having relationships with DO backed models (and vice versa)
- Support for DOs having relationships with different DOs

*though, some day, I hope to break those constraints!*

---

## Detailed Design

### What is a Durable Object?

For the sake of Cloesce, a Durable Object is:
- A class that extends the `DurableObject` class provided by Cloudflare.
- Maintains it's own D1 database and KV store.
- Can maintain web socket connections.
- Strongly consistent (i.e. requests to the same DO instance are guaranteed to be processed sequentially, and never concurrently).

Unlike a D1 database or KV/R2 bucket, DO's aren't some single global resource (though, you can design a system that way if you want!). Any number of DO instances can be created, each with it's own state and ID.

Each DO class has a `fetch` method (much like the one a Worker has) that is used to handle incoming requests.

### Defining a Durable Object Binding
A Durable Object is defined in the `env` schema declaration, much like a D1 database or KV bucket:

```cloesce
env {
    durable {
        MyDurableObject
        // other DOs...
    }
}
```

Notice the casing convention here. Unlike other bindings, a DO binding must compile to both an `env` binding and a class name, typically PascalCase:
```toml
[[durable_objects.bindings]]
name = "MyDurableObject"        # binding
class_name = "MyDurableObject"  # class name
```

For the sake of this proposal, we will keep the binding name and class name the same.

### Durable Object Backed Model

A Model can be "backed by" a Durable Object, in the exact same way it can be backed by a D1 database. The DO is the source of truth: for a Model to exist, it must exist on that Durable Object instance.

```cloesce
[use MyDurableObject]
model User {
    primary {
        id: uint 
    }

    kv (MyDurableObject, "user/{id}") {
        data: json
    }

    nav (Post::author) {
        posts
    }
}

[use MyDurableObject]
model Post {
    primary {
        id: uint
    }

    foreign (User::id) {
        author
    }
}
```

In the above code, the `User` model exists as a table in SQLite on a `MyDurableObject` instance. Additionally, the `data` field is stored in the KV store of that same DO instance, with a key of `user/{id}`. Just like in a D1 backed model, relatonships can be defined between DO backed models, granted they share the same backing DO.

### REST API Methods

Any Model backed by a DO can have REST methods defined within it's `api` block.

```cloesce
[use MyDurableObject]
model Weather {
    ...
}

api Weather {
    post helloWorld() -> string
}
```

However, the execution of a method must change. For a typical Model, the router follows the path within a Worker:
```
Request 
    -> Worker -> Route -> Validation -> Hydrate -> Dispatch
```

To register some Worker route, it looks like:

```ts
export default {
    async fetch(request: Request, env: clo.Env): Promise<Response> {
        const app = (await clo.cloesce())
            .register(Weather);
        return await app.run(request, env);
    }
};
```

A Durable Object however does not exist within a Worker, it can only be *invoked* by a Worker.

```
Request 
    -> Worker -> Route -> Redirect 
        -> DO -> Route -> Validate -> Hydrate -> Dispatch
```

Since a DO operates independently of a Worker, it needs to have its own `fetch` method, and its own app registration:

```ts
export class MyDurableObject implements clo.MyDurableObject {
    async fetch(request, env): Promise<Response> {
        const app = (await clo.cloesce())
            .register(...);
        return await app.run(request, env);
    }
}

export default {
    async fetch(request: Request, env: clo.Env): Promise<Response> {
        const app = (await clo.cloesce())
            .register(...);
        return await app.run(request, env);
    }
};
```

### ORM

Every Durable Object instance is identified by a unique ID. It can be generated randomly (a 64 character hex string), or be deterministically generated from a seed string (using `env.MyDurableObject.idFromName("some-string")`).

There are two ways to access a DO instance:
1. From outside of the DO, using `env.MyDurableObject.get(id)`, which returns a stub that can be used to send requests to that instance.
2. From inside the DO, using `this`, which allows you to directly access the instance

#### Backend API

A Model may exist on a DO, but it _is not_ the DO. The DO is just a database and compute environment that the Model uses. In this design, we will completely separate the DO instance from the Model instance (no shared `this` context, no shared fields).

For example, you can define a DO backed Model with an API like this:

```cloesce
env {
    durable {
        MyDurableObject
    }
}

[use MyDurableObject]
model Foo {
    ...
}

api Foo {
    post id(e: env) -> string
}
```

Which will translate to:
```ts
export interface Env {
    MyDurableObject: DurableObjectNamespace;
}

export abstract class MyDurableObject implements DurableObject {
    abstract fetch(request: Request, env: Env): Promise<Response>;
}

export namespace Foo {
    // ...
    type InstanceEnv = Env & { $do: MyDurableObject };


    export interface Api {
        id(e: InstanceEnv, ...): ApiResult<string>;
    }

    export namespace Orm {
        async function get(env: InstanceEnv, ...);
        async function get(env: Env, $do: string | DurableObjectId, ...);
    }
}
```

The columns of `Foo` will never include `$do`, as that is a property of the backing DO instance, not the Model itself. Instead, to access that instance, you must pass `env` to the method, which includes a `$do` property that is the DO instance on which that Model instance exists. Intentionally: `env` is not `self`.


#### Client API

Similiar to the backend, any client method will need to specify how to locate the DO instance.

```ts
// given some Foo model with PK `id` that is backed by MyDurableObject...
class Foo {
    $do: string; // the DO id
    id: number;
    // other fields...

    static async $get($do: string, id: number) {...}
    static async $list($do: string, lastSeen_id: number, limit: number) {...}
    static async $save($do: string, data: DeepPartial<Foo>) {...}

    // has a saved $do
    async myMethod() {}
}
```

A DO backed Service will act in a similar way, with an additional DO ID parameter for each method.

```ts
// given some FooService that is backed by MyDurableObject...
class FooService {
    constructor (public $do: string) {}
    async myMethod() {...}
}
```

Before, Services had entirely static methods. Now, if backed by a DO, they will need to be instantiated with a DO ID.
