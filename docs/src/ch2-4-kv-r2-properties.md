# KV and R2 Properties

D1 is a powerful relational database solution, but sometimes developers need to work with other types of storage for specific use cases. Cloesce supports integrating [Cloudflare KV](https://developers.cloudflare.com/kv/) and [Cloudflare R2](https://developers.cloudflare.com/r2/) storage directly into your Models, allowing you to leverage these storage solutions alongside D1 databases.

## Defining a Model with KV

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value storage system. Along with a key and value, KV entries can also have associated metadata.

Cloesce respects the design constraints of KV storage. For Models backed purely by KV or R2, the following are not supported:

- Relationships  
- Navigation properties  
- Migrations  


```typescript
import { Model, KV, KValue, KeyParam, IncludeTree } from "cloesce/backend";

@Model()
export class Settings {
    @KeyParam
    settingsId: string;

    @KV("settings/{settingsId}", "myNamespace")
    data: KValue<unknown> | undefined;

    @KV("settings/", "myNamespace")
    allSettings: KValue<unknown>[];

    static readonly withAll: IncludeTree<Settings> = {
        data: {},
        allSettings: {}
    };
}
```

The above Model uses only KV attributes. The `@KeyParam` decorator indicates that the `settingsId` property is used to construct the KV key for the `data` property, using string interpolation. The `@KV` decorator specifies the key pattern and the KV namespace to use.

The `data` property is of type `KValue<unknown>`, which represents a value stored in KV. You can replace `unknown` with any serializable type, but Cloesce will not validate or instantiate the data when fetching it.

The `allSettings` property demonstrates how Cloesce can fetch via prefix from KV. This property will retrieve all KV entries with keys starting with `settings/` and return them as an array of `KValue<unknown>`.

[Include Trees](./ch2-3-include-trees.md) can be used with KV Models as well to specify which properties to include when fetching data. By default, no properties are included unless specified in an Include Tree.

> *Note*: KV properties on a Model consider a missing key as a valid state, and will not return 404 errors. Instead, the value inside of the `KValue` will be set to `null`.

> *Note*: `unknown` is a special type to Cloesce designating that no validation should be performed on the data, but it is still stored and retrieved as JSON.

> *Alpha Note*: KV Models do not yet support cache control directives and expiration times. This feature is planned for a future release.

## Defining a Model with R2

[Cloudflare R2](https://developers.cloudflare.com/r2/) is an object storage solution similar to [Amazon S3](https://aws.amazon.com/pm/serv-s3/). It allows you to store and retrieve large binary objects.

Just like in KV Models, Cloesce does not support relationships, Navigation Properties, or migrations for purely R2 backed Models. 

Since R2 is used for storing large objects, the actual data of an R2 object is not fetched automatically when accessing an R2 property to avoid hitting [Worker memory limits](https://developers.cloudflare.com/workers/platform/limits/). Instead, only the metadata of the [`R2Object`](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/#r2object-definition) is retrieved. To fetch the full object data, you can use Model Methods as described in the chapter [Model Methods](./ch2-5-Model-methods.md).

```typescript
import { Model, R2, R2Object, KeyParam, IncludeTree } from "cloesce/backend";

@Model()
export class MediaFile {
    @KeyParam
    fileName: string;

    @R2("media/{fileName}.png", "myBucket")
    file: R2Object | undefined;

    static readonly withFile: IncludeTree<MediaFile> = {
        file: {}
    };
}
```

The `MediaFile` Model above is purely R2 backed. The `@KeyParam` decorator indicates that the `fileName` property is used to construct the R2 object key for the `file` property. The `@R2` decorator specifies the key pattern and the R2 bucket to use.

The `file` property is of type `R2Object`, which represents an object stored in R2. This type provides access to metadata about the object, such as its size and content type.

[Include Trees](./ch2-3-include-trees.md) can also be used with R2 backed Models to specify which properties to include when fetching data.

> *Note*: R2 properties on a Model consider a missing object as a valid state, and will not return 404 errors. Instead, the property will be set to `undefined`.

## Mixing Data Together

Cloesce allows you to combine D1, KV and R2 properties into a single Model. This provides flexibility in how you structure your data and choose the appropriate storage mechanism for each property.

```typescript
import { Model, Integer, KV, KValue, R2, R2Object, KeyParam, IncludeTree } from "cloesce/backend";

@Model()
export class DataCentaur {
    id: Integer;

    @R2("centaurPhotos/{id}.jpg", "myBucket")
    photo: R2Object;
}

@Model()
export class DataChimera {
    id: Integer;
    
    favoriteSettingsId: string;

    dataCentaurId: Integer;
    dataCentaur: DataCentaur | undefined;

    @KV("settings/{favoriteSettingsId}", "myNamespace")
    settings: KValue<unknown>;

    @R2("media/{id}.png", "myBucket")
    mediaFile: R2Object | undefined;

    static readonly withAll: IncludeTree<DataChimera> = {
        dataCentaur: {
            photo: {}
        },
        settings: {},
        mediaFile: {},
    };
}
```

In the `DataChimera` Model above, we have a mix of D1, KV, and R2 properties. The `id` property is stored in a D1 database, while the `settings` property is stored in KV and the `mediaFile` property is stored in R2.

Mixing these storage mechanisms introduces some caveats. Whenever D1 is used in a Model, it is treated as the source of truth for that Model. This means that if the primary key does not exist in D1, the entire Model is considered non-existent, even if KV or R2 entries exist for that key.

However, if a primary key exists and the KV and R2 entries do not, Cloesce considers this a valid state and will place `null` or `undefined` in those properties respectively.

Further, using `KeyParam`s in a Model with D1 limits the capabilities of the ORM, discussed [later in this chapter](./ch2-6-cloesce-orm.md). It is recommended to avoid using `KeyParam`s in Models that also use D1 Navigation Properties.