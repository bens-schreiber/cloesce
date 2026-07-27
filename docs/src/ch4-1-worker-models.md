# Worker Backed Models

A Model backed by no environment binding is referred to as a "Worker Backed Model", or sometimes "a backing-less Model".

Their fields are sourced from route parameters in a request URL.

For example:

```cloesce
model Gnat {
    route {
        id: int
        buzzing: bool
    }
}
```

The `Gnat` Model above lives within the context of a request. Its lifetime is very short.

> [!TIP]
>
> Worker Backed Models can have any number of [KV](./ch4-4-kv-fields.md) or [R2](./ch4-5-r2-fields.md) fields, using the route parameters to hydrate those fields. You can even have [navigation fields](./ch4-6-navigation-fields.md)!
