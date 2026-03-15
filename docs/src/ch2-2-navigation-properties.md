# Navigation Properties

In the previous section, we built a basic D1 backed Model with scalar properties. However, relational databases like Cloudflare D1 often involve more complex relationships between tables.

In this section, we will explore *Navigation Properties* which allow us to define relationships between different Models.

## Foreign Keys

> [!NOTE]
> A Model can only have a foreign key to another Model if it is:
> 1. D1 backed
> 2. Apart of the same database as the Model it references (lifted in future releases!)

Before diving into Navigation Properties, it's essential to understand their source: foreign keys. Foreign keys are scalar properties that reference the Primary Key of another Model, establishing a relationship between two Models.

Foreign keys directly translate to SQLite `FOREIGN KEY` constraints in the underlying D1 database.

For example, let's say we want to create a relationship between `Person` and `Dog`, where a Person can have one Dog.

```typescript
import { Model, Integer, ForeignKey } from "cloesce/backend";

@Model("db")
export class Dog {
    id: Integer;
}

@Model("db")
export class Person {
    id: Integer;

    @ForeignKey<Dog>(d => d.id)
    dogId: Integer;
}
```

The `Person` Model has a foreign key property `dogId`, which references the primary key of the `Dog` Model. This establishes a relationship where each person can be associated with one dog.

> [!NOTE]
> Cloesce does not allow circular foreign key relationships (and neither does SQLite!). 
>
> If you need to model such a relationship, consider marking a foreign key as nullable and managing the relationship at the application level.

## Navigation Properties

Inspired by [Entity Framework relationship navigations](https://learn.microsoft.com/en-us/ef/core/Modeling/relationships/navigations), Cloesce allows you to effortlessly define One to One, One to Many, and Many to Many relationships between your Models using Navigation Properties. All navigation properties are D1 backed Models themselves, or arrays of D1 backed Models.

Let's revisit our `Person` and `Dog` Models and add navigation properties to them:

```typescript
import { Model, Integer } from "cloesce/backend";

@Model("db")
export class Dog {
    id: Integer;
}

@Model("db")
export class Person {
    id: Integer;

    dogId: Integer;
    dog: Dog | undefined;
}
```

In this example, Cloesce infers that `dog` is a navigation property to `Dog`, with `dogId` as the foreign key. This allows us to access the associated `Dog` instance directly from a `Person` instance.

Cloesce has a simple inference engine that finds navigation properties, then searches for a property in the Model with the name `<navPropName><primaryKeyName>` (in any casing) to use as the foreign key.

This relationship can be explicitly expressed using the Fluent API in `cloesce.config.ts`:

```typescript
config.model(Person, builder => {
    builder

    // Property "dogId" is a foreign key referencing the model Dog,
    // using Dog's primary key "id"
    .foreignKey("dogId")
        .references(Dog, "id")
    
    // Property "dog" is one to one referencing the model Dog,
    // using the foreign key "dogId"
    .oneToOne("dog")
        .references(Dog, "dogId");
});
```

<!-- In this example, we added a navigation property `dog` to the `Person` Model using the `@OneToOne` decorator. 

This property allows us to access the associated `Dog` instance directly from a `Person` instance. The type of the navigation property is `Dog | undefined`, indicating that it may or may not be populated (elaborated on in the [Include Trees](./ch2-3-include-trees.md) section).
 -->

## One to Many

Let's modify our Models to allow a Person to have multiple Dogs:

```typescript
import { Model, Integer, ForeignKey, OneToMany } from "cloesce/backend";

@Model("db")
export class Dog {
    id: Integer;

    ownerId: Integer;
    owner: Person | undefined;
}

@Model("db")
export class Person {
    id: Integer;
    dogs: Dog[];
}
```

In this example, we added a foreign key `ownerId` to the `Dog` Model, referencing the `Person` Model. The `Person` Model now has a navigation property `dogs`, which is an array of `Dog` instances, representing all dogs owned by that person.

Cloesce can infer this relationship by finding the first property in `Dog` that references `Person` as a foreign key, and using that as the basis for the one to many relationship. If many properties reference `Person`, it will need to be explicitly stated in the Fluent API:

```typescript
config.model(Person, builder => {
    builder
    .oneToMany("dogs")
        .references(Dog, "ownerId");
    .oneToMany("otherDogs")
        .references(Dog, "otherOwnerId");
});
```

## Many to Many

Many to Many relationships have an intermediate junction table that holds foreign keys to both related Models.

```typescript
import { Model, Integer } from "cloesce/backend";

@Model("db")
export class Student {
    id: Integer;

    courses: Course[];
}

@Model("db")
export class Course {
    id: Integer;

    students: Student[];
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

A Model can have a composite primary key by using the `@PrimaryKey` decorator on multiple properties. A primary key may also be a foreign key.

```typescript
import { Model, Integer, PrimaryKey } from "cloesce/backend";

@Model("db")
class Enrollment {
    @PrimaryKey
    @ForeignKey<Student>(s => s.id)
    studentId: Integer;

    @PrimaryKey
    courseId: Integer;

    student: Student | undefined;
    course: Course | undefined;
}

@Model("db")
class Student {
    id: Integer;
    enrollments: Enrollment[];
}
```

Here, Cloesce is able to infer that Student has many Enrollments through `enrollments` because the Enrollment Model has a foreign key to Student.

```typescript
import { Model, Integer, PrimaryKey } from "cloesce/backend";

@Model("db")
class Person {
    @PrimaryKey
    firstName: string;

    @PrimaryKey
    lastName: string;

    dog: Dog | undefined;
}

@Model("db")
class Dog {
    id: Integer;

    ownerFirstName: string;
    ownerLastName: string;
    owner: Person | undefined;
}
```

Because a navigation property `owner` is defined on `Dog`, Cloesce can infer that `ownerFirstName` and `ownerLastName` together form a composite foreign key to `Person`.

Without a navigation property, Cloesce Models can only decorate a single foreign key, so the Fluent API must be used to explicitly define the composite foreign key:

```typescript
config.model(Dog, builder => {
    builder
        .foreignKey("ownerFirstName", "ownerLastName")
        .references(Person, "firstName", "lastName");
});
```