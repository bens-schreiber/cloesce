# REST APIs

By defining an API for a Model, you can specify REST endpoints that are generated as backend stubs and client methods, routed by the Cloesce runtime.

## Defining an API

Given some Model, we can define an API for it like so:

```cloesce
model Person for Db {
    primary {
        id: int
    }
}

api Person {
    get byId -> Person {
      id: int
    }

    post create -> Person {
      name: string
    }

    delete del {
      id: int
      X_Auth_Token: string
    }

    put update {
      id: int
      name: string
    }

    patch updatePartial {
      id: int
      name: string
    }
}
```

The above code defines an API for the `Person` Model:

| Verb   | Route                   | Result            |
| ------ | ----------------------- | ----------------- |
| GET    | `/Person/byId`          | `Person` instance |
| POST   | `/Person/create`        | `Person` instance |
| DELETE | `/Person/del`           | `void`            |
| PUT    | `/Person/update`        | `void`            |
| PATCH  | `/Person/updatePartial` | `void`            |

All of the above methods are _static_. They do not hydrate an instance of that Model implicitly.

> [!TIP]
>
> It is heavily recommended to use [Data Sources](./ch5-0-data-sources.md) to define generic `get`, `list` and `save` methods instead
> of defining them in an API.
>
> Every Data Source will generate a corresponding API method for the client by default.

### `[header]` tag

API methods accept all parameters in the request body by default. If you want to accept a parameter from the request headers instead, you can tag that parameter with `[header]`:

```cloesce
api Person {
    delete del {
      id: int

      [header]
      X_Auth_Token: string
    }
}
```

Headers may use `Pascal_Snake_Case` to indicate that it should be parsed as `X-Auth-Token` in the request headers.

### Generated Code

After running `cloesce compile`, the above API definition could be implemented in TypeScript as follows:

```ts
import { Api } from "@cloesce/backend.js";

export default {
  byId(id) {
    // ...
  },

  create(name) {
    // ...
  },

  del(id) {
    // ...
  },

  update(id, name) {
    // ...
  },

  updatePartial(id, name) {
    // ...
  },
} satisfies Api.Person.Of;

// alternatively, each API could be implemented individually:
export const byId: Api.Person.byId = (id) => {
  // ...
};
```

Like with most RPC frameworks, the API implementation must be registered so it can be dispatched to on a matching request. A missing implementation results in a `501 Not Implemented` response.

```ts
import { CfEnv, createApp, Person } from "@cloesce/backend.js";
import person from "./person.js";

// src/index.ts
export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = createApp().worker(env).register(Person, person);

    return app.run(request);
  },
};
```

### Registering Durable Object APIs

Durable Objects receive forwarded requests from Workers by the Cloesce runtime, and need their own app registration:

```ts
import { createApp, CfEnv } from "@cloesce/backend";
import person from "./person.js";

export class MyDurable extends DurableObject<CfEnv> {
  private base = createApp().durable(this).register(Person, person);

  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}
```

## Instance Methods

Many API endpoints start out by:

- Selecting a row from a database
- Seeing if it exists
- Returning a `404 Not Found` if it doesn't exist
- Operating on the row if it does exist

Cloesce provides a shortcut for this common pattern with _instance methods_.

An instance method is an API method that calls some Data Source `get` method to hydrate an instance of a Model, and then passes that instance to the API method implementation.

For example:

```cloesce
model Person for Db {
    primary {
        id: int
    }
}

api Person {
    self get myself -> Person { }
}
```

The above example is hydrated with `Person`'s default Data Source `get` method, which will retrieve the `Person` instance by its primary key `id`. If the instance does not exist, a `404 Not Found` response will be returned to the client.

It can be implemented in TypeScript like so:

```ts
import { Api } from "@cloesce/backend.js";

export const myself: Api.Person.myself = (self) => self;
```

The `self` parameter is a flat object containing all of the fields of that `Person` instance returned by the (default) Data Sources `get` method.

### Using a Custom Data Source

By default, all API methods will use the [Default Data Source](./ch5-1-overview.md#default-data-source) to hydrate the `self` instance.

Specify a custom data source like so:

```cloesce
model Person for Db {
    primary {
        id: int
    }

    r2 Bucket::avatars(id) {
        avatar
    }
}

source WithoutAvatar for Person {
    include {
        // Empty!
    }
}

api Person {
    self(WithoutAvatar) get myself -> Person { }
}
```

In the above code, the `myself` API method will use the `WithoutAvatar` data source to hydrate the `self` instance, which excludes the `avatar` field.

Any API method can be hydrated with any Data Source (for the same Model).

## `[internal]` Models

A Model may be sensitive and confined to only the backend of your application by using the `[internal]` tag.

This will prevent any API from accepting that Model as a parameter or returning it as a result, and will not let any public Model compose a relationship with that Model.

No interface will reach the generated client. However, _static APIs_ may be created for that Model, exposing only the methods you want to the client:

```cloesce
[internal]
model UnHashedPassword for Db {
  primary {
    id: int
  }

  column {
    password: string
  }
}

api UnHashedPassword {
  get isThisMyPassword -> bool {
    id: int
    password: string
  }
}
```

Because one static API method is defined, the client will be able to call `UnHashedPassword.isThisMyPassword` with an `id` and `password`, and receive a boolean result.

The fields of `UnHashedPassword` will not be exposed to the client.

## Execution Context

[Durable Objects](./ch3-3-durable-objects.md) do not define just an area for storing data, but a single threaded execution context.

Any method may be executed in the context of a Durable Object using [Dependency Injection](./ch6-3-dependency-injection.md). For example:

```cloesce
durable CounterDo {
    shard {
        tenant: string
    }
}

model Counter {}
api Counter {
  put increment -> int {
    tenant: string

    inject { CounterDo::tenant }
  }
}
```

Because `increment` injects an instantiated instance of `CounterDo`, any code within `increment` will be executed in the context of that Durable Object, allowing you to safely manipulate data stored in that Durable Object without worrying about race conditions.

> [!IMPORTANT]
> Only one instance of a Durable Object can be injected into a method at a time, since each instance represents a single threaded execution context.

> [!NOTE]
> If a Durable Object has no shard keys, it is effectively a singleton, and can be injected as:
>
> ```cloesce
> put method {
>    inject { CounterDo::{} }
> }
> ```

> [!NOTE]
> Injecting the Durable Object namespace is different than injecting an instance of that durable object.
>
> For example, `inject { CounterDo }` would inject the namespace, allowing you to create and manage instances of that Durable Object within your method, but not execute code within the context of any particular instance.

### Data Source Execution Context

A Data Source may execute in the context of a Durable Object with the same syntax as an API method.

This means that any API method that uses that Data Source to hydrate `self` will also execute in the context of that Durable Object.

It is illegal to inject a Durable Object into an API method that hydrates `self` from a Data Source that already injects that same Durable Object. The police will be called, and you will be fined $500.

By default, the `get` method of a Data Source injects its host Model's Durable Object.

```cloesce
source Default for Counter {
    get {
      [instance]
      tenant: string

      inject { CounterDo::tenant }
    }
}

source OutsideContext for Counter {
    get {
      [instance]
      tenant: string
    }
}

api Counter {
    // Executed inside of CounterDo
    self get myself -> Counter { }

    // Executed outside of CounterDo
    self(OutsideContext) get outside -> Counter { }
}
```

## Streams

If a JSON body is defined in an API method, Cloesce will parse and validate the body before passing it to the method implementation. This is suitable for most use cases, but for certain scenarios such as file uploads or real-time data processing, you may want to handle the request body as a stream.

```cloesce
model File {
    primary {
        id: int
    }
}

api File {
    post upload -> File {
      file: stream
    }
    get download -> stream {
      id: int
    }
}
```

The above code defines two API methods for the `File` Model:

- `POST /File/upload` - Accepts a streaming file upload and returns a `File` instance
- `GET /File/download` - Returns a streaming response for downloading a file by its ID

The implementation of the `upload` method would need to handle the incoming stream appropriately by inspecting the [ReadableStream](https://developers.cloudflare.com/workers/runtime-apis/streams/readablestream/) passed in as the `file` parameter. Similarly, the `download` method would need to return a stream that can be consumed by the client for downloading the file.

> [!NOTE]
> By using `stream` in a request body, you forgo the ability to have any other parameters in the body, other than the stream itself and HTTP headers.
>
> For example, the following will not compile:
>
> ```cloesce
> post upload -> File {
>   file: stream
>   name: string
> }
> ```
>
> Parameters tagged with `[header]` are still allowed, since they are not part of the request body.

## HttpResult

Any method may return an `HttpResult` to indicate a success or failure of the API call. Failures may include only a status code and message, while successes may include a status code, data, and headers.

If the result of an API method is not an `HttpResult`, Cloesce will automatically wrap the result in an `HttpResult.ok` with a `200 OK` status code and a `Content-Type` of `application/json`.

Both the backend and frontend utilize the `HttpResult` type to represent the result of a REST API call. This type encapsulates the success or failure of the API call, along with any relevant data or error information.

The `HttpResult` type is defined as follows:

```ts
export class HttpResult<T = unknown> {
  public constructor(
    public ok: boolean,
    public status: number,
    public headers: Headers,
    public data?: T,
    public message?: string,
    public mediaType?: MediaType,
  ) {}

  /**
   * Return some OK result with the given status, data, and headers.
   */
  static ok<T>(status: number, data?: T, init?: HeadersInit): HttpResult<T>;

  /**
   * Return a failure result with the given status, message, and headers.
   * No body may be attached.
   */
  static fail(status: number, message?: string, init?: HeadersInit): HttpResult<never>;
}
```

For example, with the following schema:

```cloesce
model Garfield for Db {
    primary {
        id: int
    }
}

api Garfield {
    get byId -> Garfield {
      id: int
    }
}
```

The implementation of the `byId` method could return an `HttpResult` like so:

```ts
import { Api } from "@cloesce/backend.js";

export const byId: Api.Garfield.byId = (id) => {
  const today = new Date();
  const isMonday = today.getDay() === 1;

  if (isMonday) {
    return HttpResult.fail(503, "Garfield hates Mondays");
  }

  return HttpResult.ok(200, { id });
};
```
