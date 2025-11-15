# Thoughts on R2, Durable Objects and Cloesce Services

Up until now, the only focus for Cloesce has been D1 and Workers. We've introduced Models, which are abstractions over D1 and Workers (a model represents a table, it's method endpoints). However, the goal of Cloesce is to tie the domains of full stack web development together, which includes all of Cloudflares developer platform.

In v0.0.5, we will introduce R2 and Durable Objects to Cloesce, which are not necessarily contained by Models.

## Services

An important primitive we will need to introduce to Cloesce are Workers endpoints that are not tied to a Model (and thus not tied to a D1 table). We will call these Services, a group of methods under a namespace which is also a closure of injected dependencies.

For example:

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

Exposed static methods will be supported, but maybe we should have a warning along the lines of "why are you doing this", since there isn't really a good reason.

Attributes on a FooService _must_ be dependency injected, and we will assume that is the case by default.

```ts
@Service
export class FooService {
    env: Env;

    async foo(...) {
        this.env.db ...
        this.bar ...
    }

    static bar(...) {
        // static, can't use env or bar
    }
}
```

Services will all be apart of the default dependency injection container, allowing them to be injected in both Service and Model methods.

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

### Client Side

Since all attributes of a Service are injected, all methods will be static on the client class:

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

We've defined services in such a way that it aligns closely with how Models are routed.

A new important step will be initializing and injecting all services, meaning we construct every object in topological order and insert it into the DI container.

We will also want to rename the `Model Middleware` to `Namespace Middleware` (or some better name), as it should apply to both Models and Services.

Lastly, Model Hydration will have to be more generic, hydrating a Model or a Service.

![Cloesce Router Interface](../../assets/Cloesce%20Router%20Interface%20with%20Services.drawio.png)

## Blobs and R2

R2 is Cloudflares object storage platform. Objects are stored under a namespace called a bucket, each having their own unique key. Keys are completely custom and not decided by Cloudflare. Objects are expected to be uploaded as a stream of bytes, which can be done through the Wrangler library's R2 API.

D1 also supports storing objects under SQLite, using the `Blob` type. Before looking at how we can support R2, we need to create a Cloesce `Blob` type to our grammar, capable of being transmitted over HTTP from client to worker, such that it can be used for D1 and R2.

### Blob CIDL Type

We will need to introduce a CIDL Type `Blob`, which indicates to our compiler that this value must be both uploaded (from the client) and received (from the worker) differently than others.

Currently, all Cloesce data flows as JSON objects, under `application/json`. JSON is meant for structured data, under a strict text format. Large file uploads are typically done over `multipart/form-data`.

#### Only Blob

Let's imagine a method takes a Blob as an argument (assume this is a Cloesce type as well as a TS type):

```ts
@POST
fooBlob(blob: Blob) {}
```

This is simple enough, our client needs to send a request with FormData:

```ts
const blob = new Blob(["hello world"], { type: "text/plain" });
const form = new FormData();
form.append("blob", blob, "hello.txt");

const response = await fetch("https://example.com/upload", {
  method: "POST",
  body: form,
});
```

Then, our worker must extract the form data and call the method:

```ts
const data request.formData();
const blob = form.get("blob");
fooBlob(blob);
```

#### Blob alongside JSON

It's reasonable to have methods that have both Blobs and JSON, ex:

```ts
@POST
fooBlob(blob: Blob, obj: SomeObj, color: string) {}
```

We will have to use a mix of formData and JSON to accomplish this upload. We will follow the format of putting all JSON data under the FormData key `json`:

```ts
const blob = new Blob(["hello world"], { type: "text/plain" });
const form = new FormData();
form.append("blob", blob, "hello.txt");
form.append("json", JSON.stringify({
    obj: {...}
    color: "..."
}))

const response = await fetch("https://example.com/upload", {
  method: "POST",
  body: form,
});
```

```ts
const data request.formData();
const blob = form.get("blob");
const json = JSON.parse(form.get("json"));
fooBlob(blob, json.obj, json.color);
```

#### Uploading Complex Relationships

Given a scenario:

```ts
class Parent {
  blobs: Blob[];
  children: Child[];
}

class Child {
  blobs: Blob[];
  favoriteBlob: Blob;
}
```

How do we serialize the objects into FormData and deserialize appropriately? We can map each blob to an index in a flattened blob array:

```ts
const parent = {...};
const blobs = [];
const json = JSON.stringify(parent, (k, v) => {
  if (k instanceof Blob) {
    blobs.push(k);
    return blobs.length - 1;
  }

  return v;
});

const formData = new FormData();
formData.append("blob", blobs);
formData.append("json", json);
```

Then, we can deserialize on the backend using our knowledge of the AST to map blobs back into the object.

#### Uploading only JSON

If no Blob type is in the request, we can continue as normal and upload straight JSON.
