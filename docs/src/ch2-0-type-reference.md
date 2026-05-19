# Type Reference

This section provides a reference for the types available in the Cloesce Schema Language. These types can be used to define your application's data [Models](./ch4-0-models.md), [APIs](./ch6-1-rest-apis.md), [Data Sources](./ch5-0-data-sources.md), and more.

## SQLite Compatible Types

Several areas of Cloesce require the use of only SQLite compatible types. The types include:

| Type     | SQLite Type      |
| -------- | ---------------- |
| `string` | TEXT             |
| `real`   | REAL             |
| `int`    | INTEGER          |
| `bool`   | INTEGER (0 or 1) |
| `date`   | TEXT (ISO 8601)  |
| `blob`   | BLOB             |
| `json`   | TEXT (JSON)      |

By default, all of these types are `NOT NULL` in the database. To allow `NULL` values, wrap the type in the `option` generic, e.g., `option<string>`, `option<int>`, etc.

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

| Type           | Description                                                                                                                          |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `option<T>`    | A nullable version of any type `T`                                                                                                   |
| `array<T>`     | An array of any type `T`                                                                                                             |
| `partial<T>`   | A version of a Model type `T` where all properties (recursive) are optional.                                                         |
| `kvobject<T>`  | A Cloudflare KV object, which includes metadata and a value of type `T`.                                                             |
| `paginated<T>` | A paginated list of items of type `T`, which includes the items and pagination metadata. Useful for wrapping KV and R2 prefix lists. |

### Objects

Any [Model](./ch4-0-models.md) or [Plain Old Object](./ch6-5-plain-old-objects.md) defined in your schema can be referenced as a type. For example, to have a Plain Old Object that references a Model:

```cloesce
model User {
    primary {
        id: int
    }
    name: string
}

poo Profile {
    user: User
    bio: string
}
```
