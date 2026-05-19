# Services

It is reasonable to have an [API](./ch6-1-rest-apis.md) endpoint that doesn't quite fit into a [Model's](./ch4-0-models.md) namespace, or that needs to interact with multiple Models and [Data Sources](./ch5-0-data-sources.md). For these cases, Cloesce provides a `service` block that allows you to define standalone API methods that can be implemented in the backend and called from the frontend.

## Defining a Service

To define a service, you can use the following syntax:

```cloesce
service {
    FooService
    // BarService
    // ...
}

api FooService {
    get do_foo() -> string
}
```

Any number of `service` blocks can be defined in your schema, and they can be used to group related API methods together. In the above code, we define a service called `FooService` with a single API method `do_foo` that returns a string.

Implement a Service just as you would implement an API for a Model:

```ts
import * as clo from "@cloesce/backend.js";

const FooService = clo.FooService.impl({
  do_foo() {
    return "foo";
  },
});
```
