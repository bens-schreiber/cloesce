# Data-less Models (the Service Pattern)

It is reasonable to have an [API](./ch6-1-rest-apis.md) endpoint that doesn't quite fit into a [Model's](./ch4-0-models.md) namespace, or that needs to interact with multiple Models and [Data Sources](./ch5-0-data-sources.md).

For these cases, Cloesce supports **data-less models**; models with no columns, no KV/R2 properties, no key fields, and no D1 binding. A data-less model is essentially a namespace for static API methods.

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

## Constraints

A data-less model:

- Can declare only **static** API methods (no `self` parameter).
- Cannot declare a data source — it has no D1-backed storage.
- Has no generated `Self`, `Key`, `Source`, or `Orm` namespace.

Adding any data field (column, KV, R2, key field) makes it a regular model with the full set of generated helpers.
