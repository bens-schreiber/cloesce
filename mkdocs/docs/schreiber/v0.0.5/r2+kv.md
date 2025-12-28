# Thoughts on R2 and KV Models

In the Cloesce abstract, we describe a tool that  "orchestrates the database, backend, client and infrastructure". In this version, we will change the word "database" to be "data".

For our purposes, data can be defined as anything that can be stored and retrieved in some persistent way. This includes relational databases (D1), object storage (R2), key-value stores (KV), and potentially other storage mechanisms Cloudflare may introduce (e.g., graph databases, document stores, whatever durable objects is, etc).

With that in mind, Cloesce should be capable of orchestrating not just D1 (via the established `Model` concept), but also R2 buckets and KV namespaces (and later, Durable Objects). From this version on, `Model` will refer to the surrounding paradigm of data orchestration, introducing R2 Models and KV Models as first-class citizens alongside D1 Models.

## KV Models

Cloudflare KV is a simple persistent storage platform capable of associating a key (which must be a string) with a value (which can be text, json, bytes, etc). Additionally, JSON metadata can be stored with each key. KV is schema-less, meaning you can throw any value into any key and face no problems. Cloesce will not try to create a schema layer over KV (though it would be interesting to explore some kind of key format protocol, TBD).

Even though KV is schema-less, the client should still be able to expect data to come in some kind of format (even a format that means no format). To make this work, KV Models will take in a generic type that the frontend will expect. We will also introduce a new `JsonValue` type to the CIDL, meaning "I don't know what the format of this is but it is JSON".

Unlike D1 models, KV Models will have no attributes. This is because the only "attributes" are key, value and metadata. Hydration of a KV Model will be simple (compared to D1): call `KV_NAMESPACE.get(key)`, which will return `null` if the key doesn't exist, or the key value and metadata if it does.

When fetching from KV, a type hint must be specified `"text" | "json" | "arraybuffer" | "stream"`. Cloesce will be capable of determining the correct way to fetch your data based off the generic type passed in. A string value would be text, byte array an array buffer, anything else in JSON. Streams are a special case however, because a KV Model must be serializeable, and a stream value would impede that. Thus, if the generic type for the model is a stream, the `value` attribute will not be generated on the client (though it will exist on the backend and be hydrated as a ReadableStream).

Below is the proposed v0.0.5 implementation:


```ts
/** KV BASE CLASS */
class KVModel<V> {
    key: string;
    value: V; // V must be a serializeable CIDL Type
    metadata: unknown;
}
```

```ts
/** BASIC EXAMPLE*/
@KV("MY_KV_NAMESPACE")
class Config extends KVModel<Json> {

    // This could be done with @CRUD(["SAVE"])
    @POST
    static post(@Inject kv: KVNamespace, value: MyConfigDto, metadata: unknown): Config {
        kv.put("config-key", value, { metadata: metadata });
        return this;
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


## R2 Models

```ts

// Cloudflare response to a `head` query 
class R2Object {
  key: string;
  version: string;
  size: number;
  etag: string;
  httpEtag: string;
  checksums: R2Checksums;
  uploaded: Date;
  httpMetadata?: R2HTTPMetadata;
  customMetadata?: Record<string, string>;
  range?: R2Range;
  storageClass: string;
  ssecKeyMd5?: string;
}

/** R2 BASE CLASS */
class R2Model {
  head: R2Object;

  // NOTE: Does not exist on the client.
  value: ReadableStream;
}
```

```ts
/** BASIC EXAMPLE*/
@R2("MY_R2_BUCKET")
class Picture extends R2Model {
    @POST
    static async post(@Inject r2: R2Bucket, key: string): Promise<Picture> {
        await r2.put(key, ""); // Empty body
        const head = await r2.head(key);
        if (head === null) {
            throw new Error("r2 failed");
        }

        return { head };
    }

    @PUT
    async put(stream: Stream) {
        await r2.put(this.head.key, stream);
    }

    @POST
    async getValue(): Stream {
        return this.value;
    }

    // This could be done with @CRUD(["GET"])
    @GET
    static async get(@Inject r2: R2Bucket, key: string): Promise<Picture | null> {
        const res = await r2.get(key);
        if (res === null) {
            return null;
        }

        return { ... }; // ...convert the R2ObjectBody to a Picture
    }

    // This could be done with @CRUD(["DELETE"])
    @DELETE
    async delete(@Inject r2: R2Bucket) {
        await r2.delete(this.head.key);
    }
}
```
