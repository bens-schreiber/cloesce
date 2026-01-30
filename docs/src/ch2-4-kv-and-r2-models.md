# KV and R2 Models

Cloesce supports models backed by Cloudflare's KV and R2 storage solutions in addition to D1 databases. This allows developers to choose the most appropriate storage mechanism for their application's needs.

## Defining a Model with KV

[Cloudflare KV](https://developers.cloudflare.com/kv/) is a globally distributed key-value storage system. Along with a key and value, KV entries can also have associated metadata.

Cloesce respects the design choice of a KV storage-- no relationships, navigation properites, or migrations are supported for KV models. When fetching data inside of a KV property it is not validated either, only hinted at.

```typescript
import { Model, KV, KValue, KeyParam, IncludeTree } from "cloesce/backend";
@Model()
export class Settings {
    @KeyParam
    settingsId: string;

    @KV("settings/{settingsId}", "myNamespace")
    data: KValue<unknown>;

    @KV("settings/", "myNamespace")
    allSettings: KValue<unknown>[];

    static readonly withAll: IncludeTree<Settings> = {
        data: {},
        allSettings: {}
    };
}
```
The above model is purely KV backed. The `@KeyParam` decorator indicates that the `settingsId` property is used to construct the KV key for the `data` property. The `@KV` decorator specifies the key pattern and the KV namespace to use.

The `data` property is of type `KValue<unknown>`, which represents a value stored in KV. You can replace `unknown` with a more specific type if you know the structure of the data being stored, but it will not be validated by Cloesce.

The `allSettings` property demonstrates how Cloesce can fetch via prefix from KV. This property will retrieve all KV entries with keys starting with `settings/` and return them as an array of `KValue<unknown>`.

Of course, Include Trees can be used with KV models as well to specify which properties to include when fetching data.

> *NOTE*: `unknown` is a special type to Cloesce designating that no validation should be performed on the data, but it is still stored and retrieved as JSON. If you want to store raw strings or binary data, use `string` or `Uint8Array` respectively.

> *ALPHA NOTE*: KV models do not yet support cache control directives. This feature is planned for a future release.

## Defining a Model with R2

[Cloudflare R2](https://developers.cloudflare.com/r2/) is an object storage solution similar to Amazon S3. It allows you to store and retrieve large binary objects.

Just like in KV models, Cloesce does not support relationships, navigation properties, or migrations for R2 models. Since R2 is used for large objects, data is never fetched aside from the values returned by an R2 HEAD request, though the request to fetch more data is initiated.

```typescript
import { Model, R2, R2Object, KeyParam, IncludeTree } from "cloesce/backend";
@Model()
export class MediaFile {
    @KeyParam
    fileName: string;

    @R2("media/{fileName}.png", "myBucket")
    file: R2Object;

    static readonly withFile: IncludeTree<MediaFile> = {
        file: {}
    };
}
```

The `MediaFile` model above is purely R2 backed. The `@KeyParam` decorator indicates that the `fileName` property is used to construct the R2 object key for the `file` property. The `@R2` decorator specifies the key pattern and the R2 bucket to use.

The `file` property is of type `R2Object`, which represents an object stored in R2. This type provides access to metadata about the object, such as its size and content type.

Include Trees can also be used with R2 models to specify which properties to include when fetching data.

## Mixing Data Together

Cloesce allows you to combine D1, KV and R2 properties into a single model. This provides flexibility in how you structure your data and choose the appropriate storage mechanism for each property.

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

In the `DataChimera` model above, we have a mix of D1, KV, and R2 properties. The `id` property is stored in a D1 database, while the `settings` property is stored in KV and the `mediaFile` property is stored in R2.

Mixing these storage mechanisms introduces some limitations. Whenever D1 is used in a model, it is treated as the source of truth for that model. This means that if the primary key does not exist in D1, the entire model is considered non-existent, even if KV or R2 entries exist for that key.

However, if a primary key exists and the KV and R2 entries do not, Cloesce considers this a valid state and will place `undefined` in those properties.

Further, using `KeyParam`s in a model with D1 provides limitations to the Cloesce ORM, discussed later in this chapter. It is recommended to avoid using `KeyParam`s in models that also use D1 properties unless absolutely necessary.