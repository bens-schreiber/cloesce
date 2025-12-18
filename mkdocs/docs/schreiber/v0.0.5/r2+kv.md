# Thoughts on R2 and KV Models

In the Cloesce abstract, we describe a tool that  "orchestrates the database, backend, client and infrastructure". This is unnecessarily limited-- why stop at just a "database" (meaning a relational SQL database)? Is it reasonable shift to "data" in general? 

For our purposes, data can be defined as: anything that can be stored and retrieved in some persistent way. This includes relational databases (D1), object storage (R2), key-value stores (KV), and potentially other storage mechanisms Cloudflare may introduce (e.g., graph databases, document stores, etc).

With that in mind, Cloesce should be capable of orchestrating not just D1 (via the established `Model` concept), but also R2 buckets and KV namespaces (and later, Durable Objects). From this version on, `Model` will refer to the surrounding paradigm of data orchestration, and we will introduce R2 Models and KV Models as first-class citizens alongside D1 Models.

## KV Models

Cloudflare KV is a simple key-value store. It is not relational, and does not support complex queries. However, it is extremely fast and globally distributed, making it ideal for caching, session storage, feature flags, and other use cases. Key sizes can be up to 512 bytes, and value sizes can reach 25MB. An additional metadata field of up to 4KB can also be stored alongside each value.

```ts
/** KV BASE CLASS */

type Json = unknown;
type KVType = string | ArrayBuffer | ReadableStream | Json;

class KVModel<V extends KVType> {
    key: string;
    value: V;
    metadata: unknown;
}
```

```ts
/** BASIC EXAMPLE*/

@KV("MY_KV_NAMESPACE")
class Config extends KVModel<Json> {

    // This could be done with @CRUD(["SAVE"])
    @POST
    static post(@Inject kv: KVNamespace, value: MyConfigDto, metadata: string) {
        kv.put("config-key", value, { metadata: metadata });
    }

    // This could be done with @CRUD(["GET"])
    @GET
    get(): Config {
        // key, value, metadata are supplied by the Cloesce Routers hydration
        return this;
    }

    // This could be done with @CRUD(["DELETE"])
    @DELETE
    delete(@Inject kv: KVNamespace) {
        kv.delete("config-key");
    }
}
```

Unlike D1, KV has no schema, but simply: `key, value, metadata`. Keys must always be strings, values can be a string, ArrayBuffer, ReadableStream, or JSON. Metadata is optional and can be any JSON-serializable object. There will be no migrations or runtime schema validation for KV Models.

KV Models will have both static and instance methods. Static methods will be used for operations that do not require an existing key (e.g., saving a new value), while instance methods will be used for operations on existing keys (e.g., calling a method on a model). The Cloesce Router will handle hydration of KV Models by fetching the value and metadata from the KV namespace based on the key, potentially throwing a 404 if the key does not exist.

KV Models are serializeable, and can be passed as arguments or return values in any method, with an appropriately generated type on the client side.

There should be no reason to limit to a one-model-per-namespace approach, so the following would be valid:

```ts
@KV("MY_KV_NAMESPACE")
class KVModelA extends KVModel<string> {
    // ...
}

@KV("MY_KV_NAMESPACE")
class KVModelB extends KVModel<Json> {
    // ...
}
```

Some features like setting a default TTL for keys will be added in future versions, along with integration with D1 Models (navigation properties based off keys stored in D1).

## R2 Models

Cloudflare R2 is an large object storage service where objects up to 5TB can be stored in a namespace called a bucket. R2 is ideal for storing large files, media assets, backups, and other unstructured data. R2 objects are stored as key-value pairs, where the key is the object name (string) and the value is the object data (binary). Along with the binary data, R2 supports JSON metadata as well as custom HTTP headers.

Sending large binary data is supported in [two ways](./services+media-types.md#blobs): as a buffered base64 `Blob` or as a `ReadableStream`. Note that a Cloudflare Worker can only handle up to 128MB of memory, so streaming is preferred for very large files. Another noteable way to interact with R2 data is through signed upload and download URLs via the Amazon S3-compatible API.

```ts
/** R2 BASE CLASS */

class R2Model {
    head: R2Object; // cloudflare defined type populated from an R2 getHead call
}
```

```ts
/** BASIC EXAMPLE*/

@R2("MY_R2_BUCKET")
class Picture extends R2Model {
    @POST
    static async post(@Inject r2: R2Bucket, stream: Stream) {
        const object = await r2.put("picture-key", stream, {
            httpMetadata: {
                contentType: "image/png",
            },
            customMetadata: {
                uploadedBy: "user123",
            },
        });
        return object;
    }

    @GET
    getValue(@Inject r2: R2Bucket): Stream {
        return await r2.get(super.head.key).body;
    }

    // This could be done with @CRUD(["HEAD"])
    @HEAD
    head(): R2Object {
        return this;
    }

    // This could be done with @CRUD(["DELETE"])
    @DELETE
    async delete(@Inject r2: R2Bucket) {
        await r2.delete(super.head.key);
    }
}
```

Like KV Models, R2 Models have no schema but instead a fixed structure returned from a `HEAD` call to fetch object metadata. No attributes are allowed on R2 Models, since R2 objects are unstructured binary data. There will be no migrations or runtime schema validation for R2 Models. R2 Models are serializeable, and can be passed as arguments or return values in any method, with an appropriately generated type on the client side.

Like all models, R2 Models will have both static and instance methods. Static methods will be used for operations that do not require an existing object (e.g., uploading a new object), while instance methods will be used for operations on existing objects (e.g., fetching or deleting an object). The Cloesce Router will handle hydration of R2 Models by fetching the object data and metadata from the R2 bucket based on the object key, potentially throwing a 404 if the object does not exist.

Because R2 objects can be very large, values will not be fetched automatically during hydration. Instead, only the `head` property (of type `R2Object`) will be populated. Developers can then choose to fetch the object data on demand using instance methods.

CRUD methods for R2 are interesting. The two most simple to implement would be `HEAD`, which returns what the Cloesce Router hydrates, and `DELETE` which deletes the object based on the key. Other operations don't have a standard form. For instance, would `GET` return the entire object data as a stream? As a buffer? As a signed download URL? Similiar problems arise for `POST`/`PUT` operations. For now, we will leave these decisions up to the developer to implement in custom methods, but future versions may introduce standard CRUD behaviors for R2 Models.

Future featues for R2 Models include support for generating signed upload and download URLs, as well as integration with D1 Models (storing R2 object keys in D1 columns).