# Model Methods

To Cloesce, models are more than just data containers; they are also the API through which the client interacts with the backend. In this section, we will talk about how the Cloesce runtime routes and hydrates models.

## Static and Instance Methods

A model class in Cloesce may have both static and instance methods. A static method is the most simple, it simply exists on the same namespace as the model class itself. An instance method exists on an actual instance of the model. Static and instance methods must be decorated with a HTTP Verb such as `@GET`, `@POST`, etc. to be visible to the runtime.

```typescript
import { Model, Integer, HttpResult } from "cloesce/backend";
@Model()
export class User {
    id: Integer;
    name: string;

    @GET
    static echo(input: string): HttpResult<string> {
        if (isBadWord(input)) {
            return HttpResult.fail(400, "I'm not saying that!");
        }

        return HttpResult.ok(`Echo: ${input}`);
    }

    @GET
    greet(): string {
        return `Hello, my name is ${this.name}.`;
    }

    foo() {
        // Not exposed as an API method
    }
}
```

After compilation via `npx cloesce compile`, the above model will have two API endpoints:
- `GET /User/echo?input=yourInput` - Calls the static `echo` method.
- `GET /User/{id}/greet` - Calls the instance `greet` method on the `User` instance with the specified `id`.

> *ALPHA NOTE*: GET methods currently do not support complex types as parameters (such as other models, arrays, etc). Only primitive types like `string`, `number`, `boolean`, etc are supported. This limitation will be lifted in future releases.

## CRUD Methods

When creating models, you will find yourself writing the same CRUD (Create, Read, Update, Delete) operations over and over again. To save you this effort, Cloesce automatically generates standard CRUD methods if included in the model decorator. These methods are exposed as API endpoints. Internally, they simply run the Cloesce ORM operations available to the developer.

```typescript
import { Model, Integer } from "cloesce/backend";

@Model(["GET", "SAVE", "LIST"])
export class User {
    id: Integer;
    name: string;
}
```

The above `User` model will have the following API endpoints generated automatically:
- `GET /User/{id}/GET` - Fetch a `User` by its primary key.
- `POST /User/SAVE` - Create or update a `User`. The `User` data is passed in the request body as JSON.
- `GET /User/LIST` - List all `User` instances.

All CRUD methods take an optional `IncludeTree` in the request to specify which navigation properties to include in the response.

> *NOTE*: R2 does not support CRUD methods for streaming the object body, instead it only sends the metadata.

> *ALPHA NOTE*: Delete is not yet supported as a generated CRUD method.

## Runtime Validation

When a model method is invoked via an API call, the Cloesce runtime automatically performs validation on the input parameters and the return value based on what it has extracted from the model definition during compilation. This ensures that the data being passed to and from the method adheres to the expected types and constraints defined in the model.

There are many valid types for method parameters in Cloesce, such as:

| Type | Description |
|------|-------------|
| `string` | String values |
| `number` | Floating-point numbers |
| `Integer` | Integer values |
| `boolean` | Boolean values (true/false) |
| `Date` | Date and time values |
| `Uint8Array` | Binary data |
| `DataSourceOf<T>` | Data source for model type `T` |
| `unknown` | JSON data of unknown structure |
| `DeepPartial<T>` | Partial version of model type `T` where anything can be missing |
| Plain Old Objects | Objects with properties of supported types |
| Model types | Custom models (e.g., `User`, `Post`) |
| Arrays | Arrays of any supported type (e.g., `string[]`, `User[]`) |
| Nullable unions | Nullable versions of any type (e.g., `string \| null`, `User \| null`) |
| `HttpResult<T>` | HTTP result wrapping any supported type `T` |
| `ReadableStream` | Stream of data |

## HttpResult

Every method response in Cloesce is converted to a `HttpResult` internally. This allows methods to have fine-grained control over the HTTP response, including status codes and headers.

## DeepPartial

Cloesce provides a special utility type called `DeepPartial<T>`, which allows for the creation of objects where all properties of type `T` are optional, and this optionality is applied recursively to nested objects. This is particularly useful for update operations where you may only want to provide a subset of the properties of a model.

## Stream Input and Output

Cloesce supports streaming data both as input parameters and return values in model methods. This is particularly useful for handling large files or data streams without loading everything into memory at once. Streams can be hinted to Cloesce using the `ReadableStream` type.

If a method parameter is of type `ReadableStream`, no other validation is performed on the input data. Additionally, no other parameters are allowed in the method signature when using a stream input (aside from injected dependencies, which are discussed later).

When a method returns a `ReadableStream`, Cloesce will return a plain `Response` on the client side, allowing for efficient streaming of data back to the client.

Cloesce allows a `HttpResult<ReadableStream>` to be returned as well, which provides the ability to set custom status codes and headers while still streaming data.


