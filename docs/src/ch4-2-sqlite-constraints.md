# SQLite Column Constraints

> [!TIP]
> All fields of a SQLite backed Model must be a [SQLite compatible type](./ch2-0-type-reference.md#sqlite-compatible-types).

This chapter provides a reference for the SQLite specific features of Models.

## Primary Key

A `primary` block is required in every SQLite backed Model. It directly translates to the SQLite `PRIMARY KEY` constraint.

```cloesce
model User for Db {
    primary {
        id: int
    }
}
```

Note that by default, primary key fields are `NOT NULL`, `UNIQUE`, and `AUTOINCREMENT` (for integer fields).

### Composite Primary Key

Any number of fields can be in the `primary` block, or, any number of `priamry` blocks can exist, defining a composite primary key.

For example, the following `User` Model has a composite primary key consisting of an `id` field and an `email` field:

```cloesce
model User for Db {
    primary {
        id: int
        email: string
    }
}

// Equivalent to:
model User for Db {
    primary {
        id: int
    }

    primary {
        email: string
    }
}
```

## Foreign Key

The `foreign` block allows you to define foreign key relationships between Models. It translates to the SQLite `FOREIGN KEY` constraint.

```cloesce
model Dog for Db {
    primary {
        id: int
    }
}

model Person for Db {
    primary {
        id: int
    }

    // Person has a foreign key relationship to Dog's field `id`
    // through its own field `dogId`.
    foreign Dog::id {
        dogId
    }
}
```

Foreign key fields inherit the type of the field they reference. In the above example, `Person::dogId` is of type `int` because it references `Dog::id`, which is of type `int`. Foreign key fields are also `NOT NULL` by default, but they do not have to be unique.

### Optional Foreign Key

To allow `NULL` values in a foreign key field, use the `optional` modifier:

```cloesce
model Person for Db {
    primary {
        id: int
    }

    foreign Dog::id optional {
        dogId
    }
}
```

### Composite Foreign Key

A Model can have a composite primary key by listing multiple fields in a primary block.

Similarly, a Model can have a composite foreign key by listing multiple fields in a foreign block.

```cloesce
model Person for Db {
    primary {
        firstName: string
        lastName: string
    }
}

model Dog for Db {
    primary {
        id: int
    }

    foreign (Person::firstName, Person::lastName) {
        ownerFirstName
        ownerLastName
    }
}
```

### Foreign Primary Key

A field can be both a primary key and a foreign key at the same time. This is useful for representing many-to-many relationships:

```cloesce
model Enrollment for Db {
    primary {
        foreign Student::id {
            studentId
        }

        foreign Course::id {
            courseId
        }
    }
}

model Student for Db {
    primary {
        id: int
    }
}

model Course for Db {
    primary {
        id: int
    }
}
```

## Unique Constraint

The `unique (field1, field2, ...)` declaration adds a unique constraint over one or more
existing fields on a Model. It translates to the SQLite `UNIQUE` constraint. A field may participate in any number of unique constraints.

```cloesce
model User for Db {
    primary {
        id: int
    }

    column {
        email: string
        username: string
    }

    foreign Profile::id {
        profileId
    }

    foreign Dog::id {
        dogId
    }

    // The combination (email, profileId, dogId) must be unique.
    unique (email, profileId, dogId)

    // `username` must be unique on its own.
    unique (username)

    // `dogId` must also be unique on its own
    unique (dogId)
}
```
