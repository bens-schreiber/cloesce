# Data Sources

Models can be composed of a lot of different kinds of data.

- You may want to retrieve only a subset of the relationships for a Model.
- You may want to write queries to filter, sort, order, paginate, and even authenticate and authorize access to data.

Unlike other ORMs, Cloesce is **not** a general purpose query builder.

Instead, when you need business logic, you define a **Data Source**: stubs implemented in the runtime.

This chapter provides a reference for how to write Data Sources in Cloesce, which are the building blocks for all data retrieval in your application.
