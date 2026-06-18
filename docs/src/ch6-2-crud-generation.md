# CRUD Generation

> [!NOTE]
> The `delete` operation is not currently supported, but will be added in a future release.

Creating the same CRUD operations for each Model can be tedious. Cloesce provides a way to automatically generate these operations based on your Model definitions and Data Source configurations.

For every public Data Source defined on a Model, Cloesce will utilize the `get`, `save`, and `list` methods of that Data Source to generate CRUD API endpoints for that Model.

## Get

By default, the `get` operation retrieves a single record by its [primary key](./ch4-2-sqlite-constraints.md#primary-key), [shard fields](./ch3-3-durable-objects.md) and [route fields](./ch4-3-kv-fields.md#route-fields). For example:

```cloesce
[crud get]
model Person for PersonDo(tenant) {
    primary {
        id: int
    }
}

source Custom for Person {
    get(special_id: int)
}
```

The above schema will generate two API methods:

- `GET /Person/$get`: Accepts arguments `tenant` and `id`, hydrates with the [Default Data Source](./ch5-1-overview.md#default-data-source), and returns a `Person` instance if a record is found

- `GET /Person/$get_Custom`: Accepts argument `special_id`, hydrates with the Custom Data Source, and returns a `Person` instance if a record is found

## List

> [!IMPORTANT]
> The `list` operation can only be used if your Model does not have _any_ [route fields](./ch4-3-kv-fields.md#route-fields).

The `list` operation retrieves multiple records. By default, it will use a seek based pagination strategy. For example:

```cloesce
[crud list]
model Person for Db {
    primary {
        id: int
    }
}

source OffsetPagination for Person {
    list(offset: int, limit: int)
}
```

The above schema will generate two API methods:

- `GET /Person/$list`: Accepts arguments `limit` and `lastSeen_id`, hydrates with the Default Data Source, and returns a paginated list of `Person` instances

- `GET /Person/$list_OffsetPagination`: Accepts arguments `offset` and `limit`, hydrates with the Custom Data Source, and returns a paginated list of `Person` instances

## Save

The `save` operation creates or updates any record within a [Data Source's](./ch5-0-data-sources.md) [include tree](./ch5-1-overview.md#include-trees).

The only parameter `save` accepts is a [partial Model instance](./ch2-0-type-reference.md#generics), which is an object that may contain a subset of the Model's fields. For example:

```cloesce
[crud save]
model Person for Db {
    primary {
        id: int
    }
}
```

The client could then invoke a method like:

```ts
export class Person {
  // ...
  static async $save(model: DeepPartial<Person>): Promise<HttpResult<Person>> {
    // ...
  }
}

const result = await Person.$save({
  id: 1,
  name: "Alice",
});
```

### R2 Fields

If your Model contains an [R2 field](./ch4-4-r2-fields.md), the `save` operation will not be able to accept any data for that field, since the ORM is designed only for JSON serializable data. To work around this, you can define a custom [instance method](./ch6-1-rest-apis.md#instance-methods) on your Model that accepts a `stream` parameter:

```cloesce
model Person {
    route {
        id: int
    }

    r2 Bucket::photos(id) {
        avatar
    }
}

api Person {
    [inject Bucket]
    post upload_photo(self, photo: stream)
}
```

```ts
import * as clo from "@cloesce/backend.js";

export const Person = clo.Person.impl({
  async upload_photo(self, env, photo) {
    await env.Bucket.photos.put(photo);
  },
});
```
