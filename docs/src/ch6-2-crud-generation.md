# CRUD Generation

> [!NOTE]
> The `delete` operation is not currently supported, but will be added in a future release.

Creating the same CRUD operations for each Model can be tedious. Cloesce provides a way to automatically generate these operations based on your Model definitions and Data Source configurations.

Each CRUD operation corresponds to an [ORM method](./ch7-0-orm-reference.md) that is generated for your Models. You can choose to generate any combination of the following operations:

## Get

The `get` operation retrieves a single record by its [primary key](./ch4-2-d1-constraints.md#primary-key) and [key fields](./ch4-4-kv-fields.md#key-fields-and-interpolation). For example:

```cloesce
[use db]
[crud get]
model Person {
    primary {
        id: int
    }

    keyfield {
        name: string
    }
}

source Custom for Person {
    include {}

    sql get(special_id: int) {
        "
        ...
        "
    }
}
```

The above schema will generate two API methods:

- `GET /Person/$get`: Accepts arguments `id` and `name`, hydrates with the [Default Data Source](./ch5-1-overview.md#default-data-source), and returns a `Person` instance if a record is found

- `GET /Person/$get_Custom`: Accepts arguments `id`, `name`, and `special_id`, hydrates with the [Custom Data Source](./ch5-2-custom-data-sources.md), and returns a `Person` instance if a record is found

## List

> [!IMPORTANT]
> The `list` operation can only be used if your Model does not have _any_ key fields.

The `list` operation retrieves multiple records. By default, it will use a seek based pagination strategy. For example:

```cloesce
[use db]
[crud list]
model Person {
    primary {
        id: int
    }
}

source OffsetPagination for Person {
    include {}

    sql list(offset: int, limit: int) {
        "
        ...
        "
    }
}
```

The above schema will generate two API methods:

- `GET /Person/$list`: Accepts arguments `limit` and `cursor`, hydrates with the Default Data Source, and returns a paginated list of `Person` instances

- `GET /Person/$list_OffsetPagination`: Accepts arguments `offset` and `limit`, hydrates with the Custom Data Source, and returns a paginated list of `Person` instances

## Save

The `save` operation creates or updates any record within a [Data Source's](./ch5-0-data-sources.md) [include tree](./ch5-1-overview.md#include-trees).

The only parameter `save` accepts is a [partial Model instance](./ch2-0-type-reference.md#generics), which is an object that may contain a subset of the Model's fields. For example:

```cloesce
[use db]
[crud save]
model Person {
    primary {
        id: int
    }

    keyfield {
        name: string
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

If your Model contains an [R2 field](./ch4-5-r2-fields.md), the `save` operation will not be able to accept any data for that field, since the ORM is designed only for JSON serializable data. To work around this, you can define a custom [instance method](./ch6-1-rest-apis.md#instance-methods) on your Model that accepts a `stream` parameter:

```cloesce
model Person {
    primary {
        id: int
    }

    r2 (bucket, "key/{id}") {
        photo
    }
}

api Person {
    [inject bucket]
    post upload_photo(self, photo: stream)
}
```

```ts
import * as clo from "@cloesce/backend.js";

export const Person = clo.Person.impl({
  async upload_photo(self, env, photo) {
    const key = this.Key.photo(self.id);
    await env.bucket.put(key, photo);
  },
});
```
