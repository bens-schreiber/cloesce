# Worker Backed Models

A Model backed by no environment binding is referred to as a "Worker Backed Model", or sometimes "a backing-less Model".

These Models are source fields from route parameters in a request URL.

For example:

```cloesce
model Gnat {
    route {
        id: int
        buzzing: bool
    }
}
```

The `Gnat` Model above lives within the context of a request. It's life is very short.

> [!TIP]
>
> Worker Backed Models can have any number of [KV](./ch4-3-kv-fields.md) or [R2](./ch4-4-r2-fields.md) fields, using the route parameters to hydrate those fields. You can even have [navigation fields](./ch4-6-navigation-fields.md)!
