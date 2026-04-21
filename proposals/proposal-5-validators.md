# Proposal: Zod-like Validators

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-04-21
- **Last Updated:** 2026-04-21

---

## Summary

This proposal introduces a set of [Zod](https://zod.dev) inspired validator tags, allowing fine-grained validation of input data in a declarative manner. These validators can be used in addition to the existing type-based validation, providing more control over the validation process.

---

## Motivation

Cloesce relies only on type-based runtime validation when parsing JSON input. While this is sufficient for many use cases, it fails to replace the expressiveness of a full validation library like Zod. Additionally, because method dispatch occurs after hydration, basic custom validation logic must occur after the fact, forcing users to write static methods on their models to be more efficient.

```cloesce

// All CRUD methods will use the same validation logic
model Foo {
    primary {
        [length 5]
        [uppercase]
        id: string
    }
}

// APIs can use the same validation available on model fields
api Foo {
    get foo(
        self,

        [length 5]
        [uppercase]
        id: string
    ) -> Foo
}
```

---

## Goals and Non-Goals

### Goals

- Introduce a subset of Zod's validators as tags that can be used on model fields and API parameters.
- Allow composition of validators (AND chaining)

### Non-Goals
- Custom user-implemented validators (i.e., generating a backend stub to be invoked at runtime)

---

## Detailed Design

### Frontend

A Validator Tag is a special type of tag that can be applied to fields of models and plain old objects, API parameters, and Data Source parameters. They may accept arguments such as:
- Integer literals (e.g. `5`, `10`)
- Real number literals (e.g. `3.14`, `0.01`)
- String literals (e.g. `"^[a-zA-Z0-9]+$"`)
- Regex literals (e.g. `/^[a-zA-Z0-9]+$/`)

All of these literals will be lexed to a respective literal type. Regex literals use [Rust regex syntax](https://docs.rs/regex/latest/regex/#syntax), as validation is performed by the Rust backend at runtime using the `regex` crate.

The grammar for a validator tag is as follows:

```
ValidatorTag
    : '[' Identifier (Literal)* ']'
```

A tag is allowed in all of these cases:
```
model Foo {
    primary {
        [tag]
        field: Type
    }

    keyfield {
        [tag]
        field
    }

    kv (bucket, "key") {
        [tag]
        field: Type
    }

    [tag]
    field: Type
}

api Foo {
    get foo(
        self,

        [tag]
        p: Type
    ) -> Type
}

poo Bar {
    [tag]
    field: Type
}

source Default for Foo {
    include {}

    sql get(
        [tag]
        p: Type
    ) {...}

    sql list(
        [tag]
        p: Type
    ) {...}
}
```

### Available Validators

**Numerical Validators (Integer and Real)**
- [gt n]: Validates that a number is greater than n, where n is an integer or real literal.
- [gte n]: Validates that a number is greater than or equal to n, where n is an integer or real literal.
- [lt n]: Validates that a number is less than n, where n is an integer or real literal.
- [lte n]: Validates that a number is less than or equal to n, where n is an integer or real literal.

**String Validators**
- [length n]: Validates that a string has a length of n, where n is an integer literal.
- [min n]: Validates that a string has a minimum length of n, where n is an integer literal.
- [max n]: Validates that a string has a maximum length of n, where n is an integer literal.
- [uppercase]: Validates that a string is in uppercase.
- [lowercase]: Validates that a string is in lowercase.
- [regex r]: Validates that a string matches the regular expression r, where r is a regex literal.

### Inheriting Validators and Generics

Validators applied to a field will be inherited by all CRUD methods of a model, as well as any API that references that field. For example:

```cloesce
[use save]
model Foo {
    primary {
        [length 5]
        id: string
    }
}
```

will not allow any save operation to succeed if the `id` field is not exactly 5 characters long. Similarly:

```cloesce
model Bar {
    // ...

    foreign (Foo::id) {
        fooId
    }
}
```

will also require that `fooId` is exactly 5 characters long, since it references `Foo::id`.

#### Generics

- **Option<T>**: Validate the inner type `T` if the value is not null. If the value is null, skip validation.

- **Array<T>** | **Paginated<T>**: Validate each item in the array against the inner type `T`. If any item fails validation, the entire array fails validation.

- **Partial<T>**: Validate all fields in `T` that are present in the input. If a field is missing from the input, skip validation for that field.

### New CIDL Types

First, it's overdue that we rename `double` to `real`. Since Cloesce compiles to multiple languages, we don't really know what the underlying representation of a floating point number will be, and `real` is a more general term that can encompass both single and double precision floats (it is also the SQLite type for floating point numbers).

`real` and `int` will represent a number that can be positive or negative. To complement this, we will introduce two new types:
- **uint**: An unsigned integer, validated to be greater than or equal to 0 at runtime.
- **ureal**: An unsigned real, validated to be greater than or equal to 0 at runtime.

Both types will be added to the CIDL type system. In target languages with no native unsigned number types, they map to the same representation as `int` and `real` respectively, with the constraint enforced at runtime by the ORM validator.

---

## Implementation

The implementation of this proposal will involve:
1. Extending the lexer to recognize literals
2. Adding a new `ValidatorTag` node to the AST, composed within a field or parameter definition
3. Semantic analysis to ensure that validators are applied to compatible types (e.g. you can't apply `[length 5]` to an `int` field), and test regex literals for validity,
4. Extend the Cloesce AST to include validator information on fields and parameters
5. Modify the ORM `validate` function to run the appropriate validation logic based on the validators specified in the AST

No validation code will be generated, the `validate` function reads validator constraints directly from the compiled AST at runtime.

### Improving Error Messages

Currently, when the `validate` function fails, it returns a generic error message that doesn't specify which field failed validation, what it expected, or what the actual value was.

In some contexts a generic message is preferable, such as when an API is not intended for public use and the developer does not want to leak implementation details. However, in most cases, it would be more helpful to return a detailed error message that includes:
- The name of the field that failed validation
- The validators that were applied to that field
- The actual value that was provided

To this end, we can introduce a new configuration to the `cloesce.jsonc` file that allows users to specify the level of detail included in validation error messages. For example:

```jsonc
{
    "bad_request_error_detail": "detailed" // or "generic", defaults to "detailed"
}
```

The backend runtime checks this configuration when constructing the `400 Bad Request` response body on validation failure.

This means that configuration details will need to be embedded in the Cloesce AST:
```rs
enum BadRequestErrorDetail {
    Generic,

    #[default]
    Detailed
}

struct Config {
    bad_request_error_detail: BadRequestErrorDetail
}

struct CloesceAst {
    // ...
    config: Config
}
```

---

## Future Work

### Boilerplate Reduction

It will be common for users to want to apply the same validators across multiple fields. To reduce boilerplate, we can allow users to define reusable validator sets that can be applied to multiple fields. For example:

```cloesce
validator name {
    [min 2]
    [max 50]
    [regex /^[a-zA-Z]+$/]
}
```

However, I hesitate to add this because it stands to reason that this boilerplate reduction is a general feature that could be useful beyond just validators. A better fix might be a macro-language embedded in Cloesce.

### Custom Validators

There will always be validation logic that cannot be easily expressed with a set of primitive validators. Right now, the standard way to handle it is to write a static API method that performs custom validation before hydration, but that is specific to one endpoint and cannot be implicitly inherited like validators can.

We could allow users to define custom validators that generate a backend stub to be invoked at runtime. For example:

```cloesce
validator isEven()

validator dateBetween(start: date, end: date)
```

A user would have to register implementations of these validators in their backend, and the `validate` ORM function would need to be extended to "bubble up" fields that need custom validation to be invoked.
