# New API Syntax

Previously:

```cloesce
kv MyKv {
    template(a: int, b: int) -> string {
        "key/{a}/{b}"
    }
}

source FooSource {
    get someSource(
        [gt 0] value: int, 
        [len 10] Content_Type: string
    )
}

api Foo {
    [inject MyInjectable, MyDurable(value)]
    get(
        [source FooSource] self, 
        [gt 0]             value: int, 
        [len 10]           Content_Type: string
    ) -> Bar
}
```

Now:

```cloesce
kv MyKv {
    template -> string {
        a: int
        b: int
        
        "key/{a}/{b}" // (optional)
    }
}

source FooSource {
    get {
        [gt 0]
        value: int

        [len 10]
        Content_Type: string
    }
}

api Foo {
    self(FooSource) get someApi -> Bar {
        [gt 0]
        value: int

        [len 10]
        [header]
        Content_Type: string

        inject {
            MyInjectable
            MyDurable::id(value)
        }
    }
}
```

- Durable Object context now uses the `::` syntax to specify the constructor parameters, and can also use the spider form for multi-parameter constructors.
    - ex: `MyDurable::id(value)` or `MyDurable::{id1(value), id2(Content_Type)}`
    - An empty constructor can be specified with `MyDurable::{}`
- `inject` tag removed
- `source` tag removed, along with the `source { ... }` block in API method bodies. Instance methods are now marked by a leading `self` receiver on the method header, before the http verb:
    - `get someApi -> Bar` is a static method (no `self`).
    - `self get someApi -> Bar` is an instance method bound to the `Default` source.
    - `self(FooSource) get someApi -> Bar` is an instance method bound to the named source `FooSource`.
    - Bare `self` is canonical for `self(Default)`; the formatter normalizes `self(Default)` to `self`.
    - Any method-level tags precede `self`, e.g. `[tags] self(Src) get someApi -> Bar`.
- Method signatures no longer accept parameters, instead they are defined in the body of the method.
- A `header` tag is added to specify that a parameter is a header parameter.
    - Functions the same as any other parameter, but comes from the request headers instead of the body.
- KV/R2/DO-KV no longer accepts parameters in the method signature, instead they are defined in the body of the method.

Also changed:
- `vars` is now `var`
- `optional` is now `option`
- Treesitter is updated with the new syntax
    - PascalCase highlights differently than snake_case+camelCase which highlight differently than SCREAMING_SNAKE_CASE, which highlights differently than Pascal_Snake_Case. All of these are for different types of identifiers.
- All reserved keywords are removed from lexer


Pros of the new syntax:
- Less compact and easier to read, especially for methods with many parameters.
- Durable Object context is more explicit, causing less confusion about injection
- Less syntax to learn, everything follows the same pattern in Cloesce (just block definitions delimited by spaces or newlines ).