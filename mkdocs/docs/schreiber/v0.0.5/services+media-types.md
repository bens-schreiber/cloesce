# Thoughts on Cloesce Services and Blob Media Types

Up until now, the focus for Cloesce has been D1 and Workers. We've created Models, which are abstractions over D1 and Workers (a model represents a table, it's method endpoints).

In v0.0.5, we will introduce Services and Cloudflare R2, both of which are not necessarily tied to a model.

## Services

The key idea of a Service is that Workers endpoints should not be forced to exist on a Model, because that would force a SQL table to exist. A Cloesce application should even be able to run without the presence of any Models or a D1 database, running entirely from services.

Within Cloesce, a Service is just a singleton namespace for methods, which can optionally capture a closure of dependency injected values.

For example, a basic service declaration in TS would look like:

```ts
@Service
export class FooService {
    @POST
    async foo(...) {} // instantiated method

    @GET
    static bar(...) {} // static method

    foo() {} // doesn't have to be exposed as a Worker endpoint
}
```

Attributes on a FooService _must_ be dependency injected values, and we will assume that is the case by default.

```ts
@Service
export class FooService {
    env: Env;

    async foo(...) {
        this.env.db ...
        this.bar ...
    }

    bar(...) {
        // static, can't use env or bar
        // note: not sure why anyone would want a static method for a service
    }
}
```

Services should exist as dependencies in our depenency injection container. Thus, Services can reference one another:

```ts
@Service
export class FooService {
    env: Env;
    bar: BarService;

    // or equivalently
    func(@Inject bar: BarService) {
        ...
    }
}
```

Just like in Models, cyclical dependencies will have to be detected at compile time. It may be possible to allow cyclical composition through lazy instantiation, but that can be saved for another milestone.

### Client Side

There is no reason for the client API to think of Services as instances, since the attributes of a service are not accessible to the client (injected dependencies). Services will default to static calls on the client.

```ts
@Service
export class FooService {
  env: Env;
  bar: BarService;

  @POST
  foo() {}
}

// => client code
export class FooService {
  static async foo(): Promise<HttpResult<void>> {...}
}
```

### Cloesce Router Interface with Services

Since Cloesce is to be implemented in several languages, we've been building an interface that all languages will follow. Services have been defined very similarly to Models, and won't be difficult to add into the router interface because of that. A new important step will be initializing and injecting all services, meaning we construct every object in topological order and insert it into the DI container. Middleware should be capable of intercepting Services and their methods as well. Lastly, Hydration will now diverge to return a Service instance instead of always querying D1.

![Cloesce Router Interface](../../assets//Cloesce%20Router%20with%20Services.png)

## Blobs 

Developers may want to transfer more than just JSON data through Cloesce endpoints. For example, uploading images, videos, or other binary data is a common use case. Cloudflare Workers support handling binary data through `ArrayBuffer` and `ReadableStream` interfaces, which we can support with proper Cloesce grammar and generator changes. Note that D1 supports a Blob type as well, meaning we can store binary data in the database if needed (though it only supports up to 1MB per Blob, so R2 is a better option for larger files).

### Blob, Stream, Media Type

To support Blobs, we will add them into the Cloesce grammar. SQLite supports a Blob type as well, meaning it is a valid column type for a Model. The way Blobs move through a REST API is vastly different than pure `application/json` data, which is all we've been dealing with up until now. JSON is meant for structured data, under a strict text format. JSON text is all buffered into memory by default. On the other hand blob uploads are typically done as `application/octet-stream`. Combining the two requires inline-serializing Blobs to base64.

#### Incoming and Outgoing Blob

Imagine a method takes a Blob as an argument.

```ts
@POST
fooBlob(blob: Blob) {}
```

There is no JSON data, just a single blob. We can send this as an octet stream.

```ts
// Client
const blob = new Blob([someUint8Array], { type: "application/octet-stream" });

await fetch("/api/upload", {
  method: "POST",
  headers: {
    "Content-Type": "application/octet-stream",
  },
  body: blob,
});
```

When an octet stream is recieved on the server, a decision needs to be made. Do I buffer this into memory, or do I treat this as a stream of bytes? To solve this dilemma, we will split the Blob type into two: `Stream` and `Blob`.

`Blob` will be the decision to buffer into memory. This could hit the Workers memory limit. `Stream` will be the decision to accept a `ReadableStream`, which Cloudflare directly supports on Workers.

```ts
// Server
@POST
fooBlob(blob: Blob) {} // => Buffer into memory

async fooBlob(stream: Stream) {} // => Readable stream
```

This will also have to be supported in the reverse case. When outgoing blob data is sent from the server, the same logic will have to happen on the client. For example, this service will produce two different types on the client when invoked.

```ts
@Service
class BlobService {
  @GET
  async getBlob(): Blob

  @GET
  async getStream(): Stream
}
```

#### Encoding Blobs in Base64 JSON Strings

It's reasonable to have methods that have both Blobs and JSON, ex:

```ts
@POST
fooBlob(blob: Blob, obj: SomeObj, color: string) {}
```

This comes with a caveat. We _cannot_ use a Stream here, because there are other parameters in the mix. Encoding the blob to b64 will allow representing both the binary data and the metadata. This means the only way to convey this endpoint in REST is to load the buffer in memory, potentially hitting the memory limit. It's also worth noting encoding and decoding a significant binary is cumbersome. A sufficient warning should be displayed in this case.

#### Determining Endpoint Media Type

We will need to define a `MediaType ` in the CIDL: `MediaType::Octet | MediaType::Json`. Each method will have to mark its set of parameters under a `MediaType`, and its return value under a `MediaType`. We can do this in the generator portion, after intaking the `cidl.pre.json`, adding a media type to the end. The backend will then assume that the `MediaType` sent by the frontend is correct (or throw a `415`), and the client will assume that the server is sending the correct `MediaType`. Note that the server could return an error which would be text. if the server responds with the incorrect media type, we will want some fatal error to occur, as the generated code should always be synced (this should only happen if the `CIDL` was tampered with).

