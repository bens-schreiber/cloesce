# Environment Variables

Any number of environment variables can be defined in the schema, which Cloesce will ensure are placed in the Wrangler configuration and made available to the Worker at runtime.

## Defining Environment Variables

> [!NOTE]
> Variables are restricted to the same set of primitive types as [SQLite Types](./ch2-0-type-reference.md#sqlite-compatible-types).

To define an environment variable, use the `var` block in the schema:

```cloesce
vars {
    MY_VAR: string
    MY_OTHER_VAR: int
}
```

[Inject them](./ch6-3-dependency-injection.md) into an API endpoint like so:

```cloesce
api Foo {
    [inject MY_VAR]
    get foo() -> string
}
```
