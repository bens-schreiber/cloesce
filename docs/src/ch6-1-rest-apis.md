# REST APIs

Cloesce Models are not just about [defining data](./ch4-0-models.md) and [how to hydrate it](./ch5-0-data-sources.md); they also allow you to define how that data can be accessed and manipulated through APIs.

By defining an `api` block for a Model, you can specify REST endpoints that are generated as backend stubs and client methods, routed by the Cloesce runtime. This allows you to easily create a fully typed API layer for your application without having to write any boilerplate code.

## Defining an API

Given some Model, we can define an API for it like so:

```cloesce
model Person {
    primary {
        id: int
    }
}

api Person {
    get by_id(id: int) -> Person
    post create(name: string) -> Person
    delete del(id: int)
}
```

The above code defines an API for the `Person` model with three endpoints:

- `GET /Person/by_id` - Fetch a person by their ID
- `POST /Person/create` - Create a new person with a name
- `DELETE /Person/del` - Delete a person by their ID

All of the above methods are _static_, meaning they are called in the namespace of a Model rather than on an instance of a Model.

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
    }
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
model Person {
    primary {
        id: int
    }
}

api Person {
    get myself(self) -> Person
}
```

The above code defines an API method `GET /Person/:id/myself`, which can be implemented in TypeScript as follows:

```ts
import * as clo from "@cloesce/backend.js";

export const Person = clo.Person.impl({
    myself(self) {
        // `self` is an instance of `clo.Person.Self` that has been automatically hydrated by Cloesce
        return self;
    }
});
```

### Using a Custom Data Source

By default, all API methods will use the [Default Data Source](./ch5-0-data-sources.md#default-data-source) to hydrate the `self` instance. However, you can specify a custom data source with the `source` tag:

```cloesce
model Person {
    primary {
        id: int
    }

    r2 (bucket, "key/{id}") {
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

The above code defines two API methods for the `File` model:

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
  static ok<T>(status: number, data?: T, init?: HeadersInit): HttpResult<T>

  /**
   * Return a failure result with the given status, message, and headers.
   * No body may be attached.
   */
  static fail(status: number, message?: string, init?: HeadersInit): HttpResult<never>

}
```

For example, with the following schema:

```cloesce
model Garfield {
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
    }
});
```