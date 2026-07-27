# Models

A Model in Cloesce defines a structure hydrated from stores of persistent data, such as:

- [D1 databases](./ch3-2-d1.md)
- [Durable Objects](./ch3-3-durable-objects.md)
- [KV namespaces](./ch4-3-kv-fields.md)
- [R2 buckets](./ch4-4-r2-fields.md).

Models do not exist in just one layer of your full stack application: they are a first class citizen across the frontend, backend, and database layers of your application.

This chapter will cover how to define Models that utilize [Environment Bindings](./ch3-0-environment.md), and the relationships that can be defined between Models.
