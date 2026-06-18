# Services

It is reasonable to have an [API](./ch6-1-rest-apis.md) endpoint that doesn't quite fit into a [Model's](./ch4-0-models.md) namespace, or that needs to interact with multiple Models and [Data Sources](./ch5-0-data-sources.md).

To accomodate this use case, Cloesce allows you to define a Model that has no data associated with it, and only serves as a namespace for API methods. This is commonly referred to as a "Service".

## Defining a Service

A service is a `model` block with empty braces:

```cloesce
model FooService {}

api FooService {
    get do_foo() -> string
}
```

The implementation is the same as for any other model:

```ts
import * as clo from "@cloesce/backend.js";

const FooService = clo.FooService.impl({
  do_foo() {
    return "foo";
  },
});
```
