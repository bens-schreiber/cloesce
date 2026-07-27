# Type Reference

This section provides a reference for the types available in the Cloesce Schema Language.

## All Types

### Primitives

| Type       | Description                                                                                   |
| ---------- | --------------------------------------------------------------------------------------------- |
| `string`   | Basic string data                                                                             |
| `real`     | Any floating-point number                                                                     |
| `int`      | Any signed integer                                                                            |
| `bool`     | Boolean value (true or false)                                                                 |
| `date`     | Date value (ISO 8601)                                                                         |
| `blob`     | Binary large object                                                                           |
| `json`     | JSON data                                                                                     |
| `stream`   | Unbuffered binary stream of data                                                              |
| `r2object` | A Cloudflare R2 object, which includes metadata and an accessor for the object's data stream. |

### Generics

| Type          | Description                                                                  |
| ------------- | ---------------------------------------------------------------------------- |
| `option<T>`   | A nullable version of any type `T`                                           |
| `array<T>`    | An array of any type `T`                                                     |
| `partial<T>`  | A version of a Model type `T` where all properties (recursive) are optional. |
| `kvobject<T>` | A Cloudflare KV object, which includes metadata and a value of type `T`.     |

### Objects

Any [Model](./ch4-0-models.md) or [Plain Old Object](./ch6-5-plain-old-objects.md) defined in your schema can be used as a type:

```cloesce
model User for Db {
    primary {
        id: int
    }

    column {
        name: string
    }
}

poo Profile {
    user: User // A reference to the User Model
    bio: string
}
```

## SQLite Compatible Types

| Type     | SQLite Type      |
| -------- | ---------------- |
| `string` | TEXT             |
| `real`   | REAL             |
| `int`    | INTEGER          |
| `bool`   | INTEGER (0 or 1) |
| `date`   | TEXT (ISO 8601)  |
| `blob`   | BLOB             |
| `json`   | TEXT (JSON)      |

Some areas of the schema will only accept types that are compatible with SQLite. By default, all of these types are `NOT NULL` in a SQLite database.

To allow `NULL` values, wrap the type in the `option` generic, e.g. `option<string>` (which is SQLite compatible).
