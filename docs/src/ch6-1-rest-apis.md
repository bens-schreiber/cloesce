# REST APIs

Cloesce Models are not just about [defining data](./ch4-0-models.md) and [how to hydrate it](./ch5-0-data-sources.md); they also allow you to define how that data can be accessed and manipulated through APIs.

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
    get by_id(id: int) -> Person
    post create(name: string) -> Person
    delete del(id: int)
    put update(id: int, name: string) -> Person
    patch update_name(id: int, name: string) -> Person
}
```

The above code defines an API for the `Person` Model:

| Verb   | Route                 |
| ------ | --------------------- |
| GET    | `/Person/by_id`       |
| POST   | `/Person/create`      |
| DELETE | `/Person/del`         |
| PUT    | `/Person/update`      |
| PATCH  | `/Person/update_name` |

All of the above methods are _static_, meaning they are called in the namespace of a Model, but do not need to hydrate an instance of that Model.

### Transpiled Code

After running `cloesce compile`, the above API definition could be implemented in TypeScript as follows:

```ts
import * as clo from "@cloesce/backend.js";

export const Person = clo.Person.impl({
  by_id(id) {
    // ...
  },

  create(name) {
    // ...
  },

  del(id) {
    // ...
  },
});
```

While the backend code must be implemented manually, the frontend client methods are generated automatically by Cloesce based on the API definition:

```ts
// .cloesce/client.ts
export class Person {
  id: number;

  static async by_id(id: number): Promise<HttpResult<Person>> {
    // ...
  }

  static async create(name: string): Promise<HttpResult<Person>> {
    // ...
  }

  static async del(id: number): Promise<HttpResult<void>> {
    // ...
  }
}
```

## Instance Methods

With Cloesce, you can skip the step of manually hydrating and validating a Model instance. By passing the `self` keyword in the API method parameters, Cloesce will automatically hydrate the instance from the relevant data source and pass it as an argument to the method. For example:

```cloesce
model Person for Db {
    primary {
        id: int
    }
}

api Person {
    get myself(self) -> Person
}
```

The above code defines an API method `GET /Person/:id/myself`, which can be implemented in TypeScript:

```ts
import * as clo from "@cloesce/backend.js";

export const Person = clo.Person.impl({
  myself(self) {
    // `self` is an instance of `clo.Person.Self` that has been automatically hydrated by Cloesce
    return self;
  },
});
```

### Using a Custom Data Source

By default, all API methods will use the [Default Data Source](./ch5-1-overview.md#default-data-source) to hydrate the `self` instance. However, you can specify a custom data source with the `source` tag:

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
    get myself([source WithoutAvatar] self) -> Person
}
```

In the above code, the `myself` API method will use the `WithoutAvatar` data source to hydrate the `self` instance, which excludes the `avatar` field. This allows you to have different API methods that return different subsets of the Model's data based on the data source used.

## Execution Context

[Durable Objects](./ch3-3-durable-objects.md) do not define just an area for storing data, but a single threaded execution context. Any method may be executed in the context of a Durable Object using [Dependency Injection](./ch6-3-dependency-injection.md). For example:

```cloesce
durable CounterDo {
    shard {
        tenant: string
    }
}

model Counter {}
api Counter {
  [inject CounterDo(tenant)]
  put increment(tenant: string) -> int
}
```

Because `increment` injects an instantiated instance of `CounterDo`, any code within `increment` will be executed in the context of that Durable Object, allowing you to safely manipulate data stored in that Durable Object without worrying about race conditions.

> [!IMPORTANT]
> Only one instance of a Durable Object can be injected into a method at a time, since each instance represents a single threaded execution context.

> [!NOTE]
> If a Durable Object has no shard keys, it is effectively a singleton, and can be injected as:
>
> ```cloesce
> [inject Global()]
> put method()
> ```

> [!NOTE]
> Injecting the Durable Object namespace is different than injecting an instance of that durable object. For example, `[inject CounterDo]` would inject the namespace, allowing you to create and manage instances of that Durable Object within your method, but not execute code within the context of any particular instance.

### Data Source Execution Context

Any method of a [Data Source](./ch5-0-data-sources.md) can also be executed in the context of a Durable Object by using the `inject` tag within that Data Source. The default implementation of a Data Source for some Durable Object backed Model will always be executed in the context of that Durable Object.

Additionally, passing `self` to an API method will utilize the context defined in the `get` method of the Data Source. This means that if the `get` method of the Data Source is executed in the context of a Durable Object, then any API method that hydrates `self` from that Data Source will also be executed in that context. For example:

```cloesce
source Default for Counter {
    [inject CounterDo(tenant)]
    get([instance] tenant: string)
}

source OutsideContext for Counter {
    get([instance] tenant: string)
}

api Counter {
    // Executed inside of CounterDo
    get myself(self) -> Counter

    // Executed outside of CounterDo
    get outside([source OutsideContext] self) -> Counter
}
```

> [!IMPORTANT]
> An instantiated method inherits its execution context from the `get` method of its Data Source, so it must **not** also inject one explicitly. Doing so is a compile error:
>
> ```cloesce
> api Counter {
>     // Error: `self` already runs inside CounterDo via the Data Source's `get`
>     [inject CounterDo(tenant)]
>     get myself(self, tenant: string) -> Counter
> }
> ```

## Streams

Cloesce buffers the full body of an incoming request by default, which is suitable for most use cases. However, for certain scenarios such as file uploads or real-time data processing, you may want to handle the request body as a stream.

To define a streaming API method, you can use the `stream` type in the API definition:

```cloesce
model File {
    primary {
        id: int
    }
}

api File {
    post upload(file: stream) -> File
    get download(id: int) -> stream
}
```

The above code defines two API methods for the `File` Model:

- `POST /File/upload` - Accepts a streaming file upload and returns a `File` instance
- `GET /File/download` - Returns a streaming response for downloading a file by its ID

The implementation of the `upload` method would need to handle the incoming stream appropriately by inspecting the [ReadableStream](https://developers.cloudflare.com/workers/runtime-apis/streams/readablestream/) passed in as the `file` parameter. Similarly, the `download` method would need to return a stream that can be consumed by the client for downloading the file.

## HttpResult

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
    get by_id(id: int) -> Garfield
}
```

The implementation of the `by_id` method could return an `HttpResult` like so:

```ts
import * as clo from "@cloesce/backend.js";

export const Garfield = clo.Garfield.impl({
  by_id(id) {
    const today = new Date();
    const isMonday = today.getDay() === 1;

    if (isMonday) {
      return HttpResult.fail(503, "Garfield hates Mondays");
    }

    return HttpResult.ok(200, { id });
  },
});
```
