# R2 Fields

[Cloudflare R2](https://developers.cloudflare.com/r2/) is a globally distributed object storage service that allows you to store and serve large amounts of unstructured data, such as images, videos, and other media files. With Cloesce, you can easily integrate R2 into your application by defining R2 fields in your Models.

Many of the same concepts and syntax for defining KV fields in Cloesce also apply to R2 fields. Read the [KV fields chapter](./ch4-3-kv-fields.md) for more information.

## Defining an Environment Binding

To use R2 fields in your Models, you first need to define an environment binding for the R2 bucket in your Cloesce schema:

```cloesce
r2 MyBucket {
    // ...templates
    image(key: string) {
        "images/{key}"
    }
}
```

Unlike [KV fields](./ch4-3-kv-fields.md), no specific type is necessary for an R2 field declaration, as the actual value is never queried and buffered into memory in the application layer.

Read more about [R2 bindings](./ch3-1-kv-and-r2.md#r2) in the Environment chapter.

## Defining an R2 Field

> [!NOTE]
> R2 is used to store large unstructured data. For this reason, Cloesce will not query and buffer the full value of an R2 field into the worker runtime. Instead, only a `HEAD` request is made to R2 to check for existence and retrieve metadata.

A field in a Model may reference an R2 bindings template to define an R2 field:

```cloesce
r2 MyBucket {
    // ...templates
    image(key: string) {
        "images/{key}"
    }
}

model Image {
    route {
        id: string
    }

    r2 MyBucket::image(id) {
        my_image
    }
}
```

The above snippet defines a Model `Image` with an R2 field `my_image` that is stored in the bucket `MyBucket` under the key "images/{id}", where `{id}` is a placeholder that will be replaced with the actual value of the `id` route field when accessing R2.

See information about [route fields](./ch4-3-kv-fields.md#route-fields) in the KV fields chapter.

## Generated Types

### Backend

Since Cloesce does not fetch the actual value of an R2 field into the application layer, the Cloudflare standard [R2ObjectBody](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/#r2objectbody-definition) type is used for all R2 fields in the generated backend code.

Each R2 Bucket will also generate a corresponding namespace with helper functions `template`, `get`, `put`, and `list`, with similar signatures to the [KV helper functions](./ch4-3-kv-fields.md#backend-helpers) generated for KV namespaces.

### Frontend

It is possible to serialize the `r2object` type (or a Model field under the `r2` block).

Cloesce will send a subset of the full R2 `HEAD` response metadata back to the frontend, including the `key`, `version`, `size`, `etag`, `httpEtag`, `uploaded` timestamp, and any custom metadata defined on the R2 object. This allows you to work with R2 objects in the frontend without having to fetch the full object data.

```ts
export interface R2Object {
  key: string;
  version: string;
  size: number;
  etag: string;
  httpEtag: string;
  uploaded: Date;
  customMetadata?: Record<string, string>;
}
```
