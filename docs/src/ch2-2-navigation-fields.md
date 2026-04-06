# Navigation Fields

In the previous section, we built a basic D1 backed Model with scalar fields. However, when utilizing relational databases like Cloudflare D1, you often need more complex relationships between tables.

*Navigation Fields* allow us to define relationships between different Models.

## Foreign Keys

> [!NOTE]
> A Model can only have a foreign key to another Model if it is:
> 1. D1 backed
> 2. Part of the same database as the Model it references (lifted in future releases!)

Before diving into Navigation Fields, it's essential to understand their source: foreign keys. Foreign keys are scalar fields that reference the Primary Key of another Model, establishing a relationship (e.g., one-to-one, one-to-many) between the two Models.

The `foreign` block in Cloesce directly translates to a SQLite `FOREIGN KEY` constraint in the underlying D1 database.

For example, let's say we want to create a relationship between `Person` and `Dog`, where a Person can have one Dog.

```cloesce
[use db]
model Dog {
    primary {
        id: int
    }

    name: string
}

[use db]
model Person {
    primary {
        id: int
    }

    foreign (Dog::id) {
        dogId
    }
}
```

The `Person` Model has a foreign key property `dogId`, which references the primary key of the `Dog` Model. This establishes a relationship where each person can be associated with one dog.

### Optional Foreign Key

Cloesce does not allow circular foreign key relationships and will throw an error if it detects one at compile time. However, if you need to model such a relationship, you can make one of the foreign keys optional (nullable) and manage the relationship at the application level.

```cloesce
[use db]
model Person {
    primary {
        id: int
    }

    foreign (Person::id) optional {
        parentId
    }
}
```

## Navigation Fields

Inspired by [Entity Framework relationship navigations](https://learn.microsoft.com/en-us/ef/core/Modeling/relationships/navigations), Cloesce allows you to effortlessly define One to One, One to Many, and Many to Many relationships between your Models using Navigation Fields. All navigation fields are D1 backed Models themselves, or arrays of D1 backed Models.

Let's revisit our `Person` and `Dog` Models and add navigation fields to them:

```cloesce
[use db]
model Dog {
    primary {
        id: int
    }

    name: string
}

[use db]
model Person {
    primary {
        id: int
    }

    foreign (Dog::id) {
        dogId
        nav { dog }
    }
}
```

In this example, we've added a navigation field `dog` to the foreign key block to `Dog::id`. During hydration of a `Person` instance, Cloesce will automatically populate the `dog` property with the corresponding `Dog` instance based on the foreign key relationship. Mythical!

## One to Many

Let's modify our Models to allow a Person to have multiple Dogs:

```cloesce
[use db]
model Dog {
    primary {
        id: int
    }

    name: string
    foreign (Person::id) {
        ownerId
    }
}

[use db]
model Person {
    primary {
        id: int
    }

    nav (Dog::ownerId) {
        dogs
    }
}
```

In this example, we added a foreign key `ownerId` to the `Dog` Model, referencing the `Person` Model. The `Person` Model now has a navigation property `dogs`, which is an array of `Dog` instances, representing all dogs owned by that person.

## Many to Many

Many to Many relationships have an intermediate junction table that holds foreign keys to both related Models.

```cloesce
[use db]
model Course {
    primary {
        id: int
    }

    name: string

    nav (Student::courses) {
        students
    }
}

[use db]
model Student {
    primary {
        id: int
    }

    name: string

    nav (Course::students) {
        courses
    }
}
```

An underlying junction table will be automatically created by Cloesce during migration:
```sql
CREATE TABLE IF NOT EXISTS "CourseStudent" (
  "left" integer NOT NULL,
  "right" integer NOT NULL,
  PRIMARY KEY ("left", "right"),
  FOREIGN KEY ("left") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("right") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);
```

> [!NOTE]
> The left column lists the Model name that comes first alphabetically; the right column lists the one that comes after.

## Composite Keys

A Model can have a composite primary key by using the `@PrimaryKey` decorator on multiple fields. A primary key may also be a foreign key.

```cloesce
[use db]
model Enrollment {
    primary foreign (Student::id) {
        studentId
        nav { student }
    }

    primary foreign (Course::id) {
        courseId
        nav { course }
    }
}

[use db]
model Student {
    primary {
        id: int
    }

    nav (Enrollment::studentId) {
        enrollments
    }
}
```

In this example, the `Enrollment` Model has a composite primary key consisting of `studentId` and `courseId`, which are also foreign keys to the `Student` and `Course` Models, respectively. The `Student` Model has a navigation property `enrollments`, which is an array of `Enrollment` instances representing all courses a student is enrolled in.

```cloesce
[use db]
model Person {
    primary {
        firstName: string
        lastName: string
    }
}

[use db]
model Dog {
    primary {
        id: int
    }

    foreign (Person::firstName, Person::lastName) {
        ownerFirstName
        ownerLastName
        nav { owner }
    }
}
```

In this example, the `Dog` Model has a composite foreign key consisting of `ownerFirstName` and `ownerLastName`, which reference the `firstName` and `lastName` fields of the `Person` Model, respectively. The foreign key block also includes a navigation property `owner`, which will be populated with the corresponding `Person` instance during hydration of a `Dog` instance.

## Unique Constraints

Cloesce supports adding unique constraints to any column or foreign key. By default, a primary key is unique. Any field within a unique block is apart of the same unique constraint.

```cloesce
[use db]
model User {
    primary {
        id: int
    }

    unique {
        email: string
    }

    unique foreign (Group::id) {
        groupId
    }

    unique {
        foreign (OtherModel::id) {
            otherId
        }

        foreign (AnotherModel::id) {
            anotherId
        }
    }
}
```