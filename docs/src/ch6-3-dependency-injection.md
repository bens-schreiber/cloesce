# Dependency Injection

Any API method may inject [Environment Bindings](./ch3-0-environment.md), or inject custom interfaces defined in the schema.

The [Cloesce ORM](./ch7-0-orm-reference.md) is invoked through dependency injected bindings. To access any generated helper, Data Source, or API method from within an API method, you _must_ inject it at the schema level.

## Injecting Environment Bindings

To inject an Environment Binding, add the `inject` tag to the API method and specify the name of the binding you want to inject:

```cloesce
d1 { Db }

r2 Bucket {
    image {
        id: string
    }
}

var {
    SECRET: string
}

model Person for Db {
    primary {
        id: int
    }
}

api Person {
    get stuff -> Person {
        inject {
            Db
            Bucket
            SECRET
        }
    }
}
```

A generated backend stub for the `stuff` API method will include an `env` parameter with **ORM upgraded** types:

- `env.Db` will contain all Models within the `Db` binding, with all of their Data Sources and API methods invokable
- `env.Bucket` will contain all templated R2 methods for the `Bucket` binding, with read, write and list methods invokable
- `env.SECRET` will contain the value of the `SECRET` binding, as a string

See the [Cloesce ORM](./ch7-0-orm-reference.md) for more information on how to use the injected bindings.

## Defining Custom Inject Bindings

Custom values beyond Worker resources can be defined and injected into API methods.

For example, you may want to create an `Auth` dependency that any API method can inject to perform authentication and authorization checks:

```cloesce
// Define a custom interface to be injected
inject { Auth }

// Define an API method that injects the Auth interface
api Person {
    get stuff -> string {
        inject { Auth }
    }
}
```

In order for the dependency to resolve at runtime (any missing dependency is a `500` error), you must register an implementation in the backend:

```ts
// Give the Auth interface a type
declare module "./backend.js" {
  interface Auth {
    username: string | null;
  }
}

const stuff: Api.Person.stuff = (env) => HttpResult.ok(200, `my username is ${env.Auth.username}`);

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp()
      .worker(env)
      .register(Auth, { username: "john_doe" })
      .register(Person, { stuff })
      .run(request);
  },
};
```
