# Data Sources

There are many ways to retrieve a Model.

- You may want to retrieve only a subset of the relationships for a Model.
- You may want to write complex queries with filtering, sorting, ordering, pagination, and even authentication to tell some people that they can't see certain data.

Cloesce does **not** try to be a general purpose query language.

Instead, when you need business logic, you define Data Sources: stubs that you implement in your own HLL.

This chapter provides a reference for how to write Data Sources in Cloesce, which are the building blocks for all data retrieval in your application.
