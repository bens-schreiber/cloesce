# R2 Fields

[Cloudflare R2](https://developers.cloudflare.com/r2/) is a globally distributed object storage service that allows you to store and serve large amounts of unstructured data, such as images, videos, and other media files.

With Cloesce, you can easily integrate R2 into your application by defining R2 fields in your Models. Read the [R2 Bindings](./ch3-1-kv-and-r2.md#r2) section in the Environment chapter for more information on how to set up R2 bindings in your Cloesce schema.

## Defining an R2 Field

> [!NOTE]
> R2 is used to store large unstructured data. For this reason, Cloesce will not query and buffer the full value of an R2 field into the worker runtime. Instead, only a `HEAD` request is made to R2 to check for existence and retrieve metadata.

A field in a Model may reference an R2 bindings template to define an R2 field:

```cloesce
r2 MyBucket {
    image {
        key: string
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
