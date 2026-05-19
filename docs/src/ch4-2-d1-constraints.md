# D1 Column Constraints
> [!TIP]
> All fields of a D1 backed Model must be [SQLite compatible types](./ch2-0-type-reference.md#sqlite-compatible-types).

This chapter provides a reference for the D1 specific features of Cloesce Models that modify the SQL schema with constraints. Any Model that is backed by a D1 database (i.e., any Model that has a `[use DB_NAME]` tag) can use these features.

Note that the `[use]` tag may be omitted from the examples in this chapter for brevity.

## Primary Key

The `primary` block is required in every D1 backed Model. It directly translates to the SQLite `PRIMARY KEY` constraint.

```cloesce
model User {
    primary {
        id: int
    }
}
```

### Composite Primary Key

Any number of fields can be in the `primary` block, allowing for composite primary keys.

For example, the following `User` Model has a composite primary key consisting of an `id` field and an `email` field:

```cloesce
model User {
    primary {
        id: int
        email: string
    }
}

// Equivalent to:
model User {
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
model Dog {
    primary {
        id: int
    }
}

model Person {
    primary {
        id: int
    }

    // Person has a foreign key relationship to Dog's field `id`
    // through it's own field `dogId`.
    foreign (Dog::id) {
        dogId
    }
}
```

Foreign key fields inherit the type of field that they reference. In the above example, `Person::dogId` is of type `int` because it references `Dog::id`, which is of type `int`. Foreign key fields are also `NOT NULL` by default, but they do not have to be unique.

### Optional Foreign Key

> [!TIP]
> The `optional` modifier can be used to wrap any number of columns or foreign blocks, modifying all of them to allow `NULL` values.

To allow `NULL` values in a foreign key field, use the `optional` modifier:

```cloesce
model Person {
    primary {
        id: int
    }

    optional {
        foreign (Dog::id) {
            dogId
        }
    }

    // Or, use the infix notation, which is equivalent:
    foreign (Dog::id) optional {
        dogId
    }
}
```

### Composite Foreign Key

A Model can have a composite primary key by listing multiple fields in a primary block. Similarly, a Model can have a composite foreign key by listing multiple fields in a foreign block.

```cloesce
model Person {
    primary {
        firstName: string
        lastName: string
    }
}

model Dog {
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

A field can be both a primary key and a foreign key at the same time. This is useful for manually representing many-to-many relationships:

```cloesce
model Enrollment {
    primary {
        foreign (Student::id) {
            studentId
        }

        foreign (Course::id) {
            courseId
        }
    }

    // Or, equivalently, use the infix notation:
    foreign (Student::id) primary {
        studentId
    }

    foreign (Course::id) primary {
        courseId
    }
}

model Student {
    primary {
        id: int
    }
}

model Course {
    primary {
        id: int
    }
}
```

## Unique Constraint

The `unique` block allows you to define unique constraints across any number of fields in a Model. It translates to the SQLite `UNIQUE` constraint.

```cloesce
model User {
    primary {
        id: int
    }

    unique {
        email: string

        foreign (Profile::id) {
            profileId
        }

        // Infix form is also supported for foreign keys, 
        // even under a unique block
        foreign (Dog::id) unique {
            dogId
        }
    }

    unique {
        username: string
    }
}
```

The above `User` Model has four unique constraints:

1. The combination of `email`, `profileId`, and `dogId` must be unique across all rows in the `User` table.
2. `dogId` must be unique across all rows in the `User` table.
3. `username` must be unique across all rows in the `User` table.
4. `id` is unique across all rows in the `User` table by virtue of being a primary key.
