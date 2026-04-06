# KV and R2 Fields

D1 is a powerful relational database solution, but is unsuited for storing large binary objects or frequently accessed non-relational data. Cloesce supports integrating [Cloudflare KV](https://developers.cloudflare.com/kv/) and [Cloudflare R2](https://developers.cloudflare.com/r2/) storage directly into your Models, allowing you to leverage these storage solutions alongside D1 databases.

## Defining a Model with KV
> [!IMPORTANT]
> KV Models do not yet support cache control directives and expiration times. This feature is planned for a future release.

> [!NOTE]
> KV fields on a Model consider a missing key as a valid state, and will not return 404 errors. Instead, the value inside of the `KValue` will be set to `null`.

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value storage system. Along with a key and value, KV entries can also have associated metadata.

Cloesce respects the design constraints of KV storage. For Models backed purely by KV or R2, the following are not supported:

- Relationships  
- Navigation fields  
- Migrations  

```cloesce
env {
    kv {
        myNamespace
    }
}

model Settings {
    keyfield {
        settingsId
    }

    kv(myNamespace, "settings/{settingsId}") {
        data: json
    }

    kv(myNamespace, "settings/") paginated {
        allSettings: json
    }
}

```

The above model has no D1 backing, and is purely stored in KV. A `keyfield` is a special type of field that is not stored anywhere, and is used only for constructing the key for KV entries. In this case, the `settingsId` field is used to construct the key for both the `data` and `allSettings` fields.

The `data` field is of type `json`, which means that the value stored in KV will be JSON. The `allSettings` field is marked as `paginated`, which means that it will fetch all entries in the `settings/` prefix and return them as a paginated list (fetching 1000 entries at a time, the maximum allowed by Cloudflare).

[Data Source Include Trees](./ch2-3-data-sources.md) can be used with any KV field as well to specify which fields to include when fetching data.

## Defining a Model with R2

> [!NOTE]
> R2 fields on a Model consider a missing object as a valid state, and will not return 404 errors. Instead, the field will be set to `undefined`.

[Cloudflare R2](https://developers.cloudflare.com/r2/) is an object storage solution similar to [Amazon S3](https://aws.amazon.com/pm/serv-s3/). It allows you to store and retrieve large binary objects.

Just like in KV Models, Cloesce does not support relationships, Navigation Fields, or migrations for purely R2 backed Models. 

Since R2 is used for storing large objects, the actual data of an R2 object is not fetched automatically when accessing an R2 field to avoid hitting [Worker memory limits](https://developers.cloudflare.com/workers/platform/limits/). Instead, only the metadata of the [`R2Object`](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/#r2object-definition) is retrieved. To fetch the full object data, you can use Model Methods as described in the chapter [Model Methods](./ch2-5-Model-methods.md).

```cloesce
env {
    r2 {
        myBucket
    }
}

model MediaFile {
    keyfield {
        fileName
    }

    r2(myBucket, "media/{fileName}.png") {
        file
    }

    r2(myBucket, "media/") paginated {
        allFiles
    }
}
```

In the `MediaFile` Model above, the `fileName` field is used as a `keyfield` to construct the key for the R2 object. The `file` field is of type `R2Object`, which means that it will return the metadata of the R2 object stored at the key `media/{fileName}.png` in the `myBucket` bucket. The `allFiles` field is marked as `paginated`, which means that it will fetch all objects in the `media/` prefix and return their metadata as a paginated list.

[Data Source Include Trees](./ch2-3-data-sources.md) can also be used with R2 fields to specify which fields to include when fetching data.

## Mixing Data Together

Cloesce allows you to combine D1, KV, and R2 fields into a single Model. This provides flexibility in how you structure your data and choose the appropriate storage mechanism for each field.

```cloesce
env {
    kv {
        myNamespace
    }

    r2 {
        myBucket
    }

    d1 {
        db
    }
}

[use db]
model DataCentaur {
    primary {
        id: int
    }

    r2(myBucket, "centaurPhotos/{id}.jpg") {
        photo
    }
}

[use db]
model DataChimera {
    primary {
        id: int
    }

    favoriteSettingsId: string

    foreign (DataCentaur::id) {
        dataCentaurId
        nav { dataCentaur }
    }

    kv(myNamespace, "settings/{favoriteSettingsId}") {
        settings: json
    }

    r2(myBucket, "media/{id}.png") {
        mediaFile
    }
}
```

In the `DataChimera` Model above, we have a mix of D1, KV, and R2 fields. The `id` field is stored in a D1 database, while the `settings` field is stored in KV and the `mediaFile` field is stored in R2.

Mixing these storage mechanisms introduces some caveats. Whenever D1 is used in a Model, it is treated as the source of truth for that Model. This means that if the primary key does not exist in D1, the entire Model is considered non-existent, even if KV or R2 entries exist for that key.

However, if a primary key exists and the KV and R2 entries do not, Cloesce considers this a valid state and will place `null` or `undefined` in those fields respectively.

Furthermore, using `keyfield`s in a Model with D1 limits the capabilities of the ORM, discussed [later in this chapter](./ch2-6-cloesce-orm.md). It is recommended to avoid using `keyfield`s in Models that use D1 unless you have a specific use case that requires it.