# Models

A Model in Cloesce defines a structure hydrated from some sources of persistent data, such as a [D1 database](./ch3-2-d1.md), [Durable Object](./ch3-3-durable-objects.md), [KV namespace](./ch4-3-kv-fields.md), or [R2 bucket](./ch4-4-r2-fields.md).

Once defined, a Model is a first class citizen across the frontend, backend, and database layers of your application, capable of housing [API endpoints](./ch6-0-apis.md) and being serialized for the client.

In this chapter, we will explore the various features of Cloesce Models, including:

- [SQLite Backed Models](./ch4-1-sqlite-backed-model.md)
- [SQLite Column Constraints](./ch4-2-sqlite-constraints.md)
- [KV Fields](./ch4-3-kv-fields.md)
- [R2 Fields](./ch4-4-r2-fields.md)
- [Navigation Fields](./ch4-5-navigation-fields.md)
