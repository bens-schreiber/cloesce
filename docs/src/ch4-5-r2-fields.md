# R2 Fields

[Cloudflare R2](https://developers.cloudflare.com/r2/) is a globally distributed object storage service that allows you to store and serve large amounts of unstructured data, such as images, videos, and other media files. With Cloesce, you can easily integrate R2 into your application by defining R2 fields in your Models.

Just like with [KV fields](./ch4-4-kv-fields.md), R2 can integrate with D1 backed Models. Many of the same syntax and concepts apply between KV and R2 fields in Cloesce (such as key interpolation), with the main difference being that R2 is designed for large unstructured data, while KV is designed for smaller key-value pairs.

## Defining an Environment Binding

To use R2 fields in your Models, you first need to define an environment binding for the R2 bucket in your Cloesce schema. This is done using the `env` block, where you specify the R2 buckets your application will use.

```cloesce
env {
    r2 {
        my_bucket
    }
}
```

In the above example, we have defined an R2 environment binding called `my_bucket`. This binding will be used to reference the R2 bucket in our Model definitions. Cloesce will generate all necessary Wrangler configurations and typed backend code to seamlessly integrate this R2 bucket into your application.

## Defining an R2 Field

> [!NOTE]
> R2 is used to store large unstructured data. For this reason, Cloesce will not query and buffer the full value of an R2 field into the worker runtime. Instead, only a `HEAD` request is made to R2 to check for existence and retrieve metadata.

A field in a Model can exist in Cloudflare R2 by using the `r2` block, which specifies the binding and key for a field. Unlike [KV fields](./ch4-4-kv-fields.md), no specific type is necessary for an R2 field declaration, as the actual value is never queried and buffered into memory in the application layer.

```cloesce
model Image {
    keyfield {
        id: string
    }

    r2 (my_bucket, "images/{id}.jpg") {
        my_image
    }
}
```

The above snippet defines a Model `Image` with an R2 field `my_image` that is stored in the bucket `my_bucket` under the key "images/{id}.jpg".

The `{id}` in the key is a placeholder that will be replaced with the actual value of the `id` keyfield when accessing R2. See information about [keyfields](./ch4-4-kv-fields.md#key-fields-and-interpolation) in the KV fields chapter, as the same concept applies to R2 fields as well.

## Paginated List Queries

Cloesce supports paginated prefix list queries for R2 fields, using the same `paginated` block syntax as KV fields. This allows you to efficiently retrieve lists of objects stored in R2 with a common key prefix, without having to load all objects into memory at once.

```cloesce
model Image {
    keyfield {
        id: string
    }

    r2 (my_bucket, "images") paginated {
        my_image
    }
}
```

In the above example, the `paginated` block indicates that `my_image` is an R2 field that should be queried using a paginated prefix list query. When you query for `my_image`, Cloesce will automatically handle the pagination logic and return a list of objects that match the specified key prefix (e.g. `"images/foo"`, `"images/bar"`, etc.).

## Generated Types

### Backend

Since Cloesce does not fetch the actual value of an R2 field into the application layer, the Cloudflare standard [R2ObjectBody](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/#r2objectbody-definition) type is used for all R2 fields in the generated backend code.

### Frontend

It is possible to serialize `r2object` type (or a Model field under the `r2` block). In this case, Cloesce will send a subset of the full R2 `HEAD` response metadata back to the frontend, including the `key`, `version`, `size`, `etag`, `httpEtag`, `uploaded` timestamp, and any custom metadata defined on the R2 object. This allows you to work with R2 objects in the frontend without having to fetch the full object data.

```ts
export class R2Object {
  key!: string;
  version!: string;
  size!: number;
  etag!: string;
  httpEtag!: string;
  uploaded!: Date;
  customMetadata?: Record<string, string>;
}
```
