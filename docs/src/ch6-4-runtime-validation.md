# Runtime Validation

When an HTTP request is made to the Cloesce Router, the incoming data will first be matched to an existing [API](./ch6-1-rest-apis.md) implementation, and then validated against the respective API defined in the Cloesce Schema.

Each type is validated in accordance with the rules defined in the [Type Reference](./ch2-0-type-reference.md). If any validation errors occur, a `400 Bad Request` response will be returned with details about the validation errors.

In addition to this, several _Validator Tags_ are also supported for more complex validation scenarios.

## Overview

Validator Tags can be applied to any field (i.e. it follows the syntax `field: type`) in a [Model](./ch4-0-models.md), [API](./ch6-1-rest-apis.md) parameter, or [Data Source](./ch5-0-data-sources.md) parameter.

A [foreign key](./ch4-2-sqlite-constraints.md#foreign-key) field will automatically inherit all validators from the field it references. For example:

```cloesce
model User {
    primary {
        [gt 0]
        id: int
    }
}

model Post {
    primary {
        id: int
    }

    foreign (User::id) {
        user_id
    }
}
```

In the above code, the `user_id` field in the `Post` Model will automatically have the `[gt 0]` validator applied to it, since it is a foreign key referencing the `id` field in the `User` Model.

## Numerical Validators

These validators apply to the `int` and `real` types:

| Validator      | Description                                                            |
| -------------- | ---------------------------------------------------------------------- |
| `[gt value]`   | Value must be greater than `value`                                     |
| `[gte value]`  | Value must be greater than or equal to `value`                         |
| `[lt value]`   | Value must be less than `value`                                        |
| `[lte value]`  | Value must be less than or equal to `value`                            |
| `[step value]` | Value must be a multiple of `value` (where `value` must be an integer) |

## String Validators

These validators apply to the `string` type:

| Validator        | Description                                                                     |
| ---------------- | ------------------------------------------------------------------------------- |
| `[len value]`    | String length must be exactly `value`, where `value` is a non-negative integer  |
| `[minlen value]` | String length must be at least `value`, where `value` is a non-negative integer |
| `[maxlen value]` | String length must be at most `value`, where `value` is a non-negative integer  |
| `[regex r]`      | String must match the regular expression `r`                                    |

Cloesce uses Rust's `regex` crate for regular expression validation and evaluation. A regex pattern is provided as a regex literal:

```cloesce
[regex /^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/]
email: string
```
