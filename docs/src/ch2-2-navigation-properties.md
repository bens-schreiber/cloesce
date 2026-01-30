# Navigation Properties and Include Trees

In the previous section, we created a basic D1 model with scalar properties. However, real-world applications often require relationships between different models. In this section, we will explore navigation properties and include trees in D1 models, which allow us to define and manage relationships between different entities.

## Foreign Keys

Before diving into navigation properties, it's essential to understand foreign keys. Foreign keys are scalar properties that reference the primary key of another model, establishing a relationship between two models.

For example, let's say we want to define a relationship between `Person` and `Dog`, where a person can have one dog

```typescript
import { Model, Integer, ForeignKey } from "cloesce/backend";

@Model()
export class Dog {
    id: Integer;
}

@Model()
export class Person {
    id: Integer;

    @ForeignKey(Dog)
    dogId: Integer;
}
```

In the above code, we defined a `Dog` model and a `Person` model. The `Person` model has a foreign key property `dogId`, which references the primary key of the `Dog` model. This establishes a relationship where each person can be associated with one dog.

## Navigation Properties

Inspired by [Entity Framework relationship navigations](https://learn.microsoft.com/en-us/ef/core/modeling/relationships/navigations), Cloesce allows you to effortlessly define One to One, One to Many and Many to Many relationships between your models using navigation properties. All navigation properties are D1 models themselves, or arrays of D1 models.

Let's revisit our `Person` and `Dog` models and add navigation properties to them:

```typescript
import { Model, Integer, ForeignKey, OneToOne } from "cloesce/backend";

@Model()
export class Dog {
    id: Integer;
}

@Model()
export class Person {
    id: Integer;

    @ForeignKey(Dog)
    dogId: Integer;

    @OneToOne<Dog>(p => p.dogId)
    dog: Dog | undefined;
}
```

In this example, we added a navigation property `dog` to the `Person` model using the `@OneToOne` decorator. This property allows us to access the associated `Dog` instance directly from a `Person` instance. The type of the navigation property is `Dog | undefined`, indicating that it may or may not be populated (discussed in Include Trees).

Just like in Entity Framework, omitting decorators is possible when proper naming conventions are followed. The above code can be reduced to:

```typescript
import { Model, Integer } from "cloesce/backend";

@Model()
export class Dog {
    id: Integer;
}

@Model()
export class Person {
    id: Integer;

    dogId: Integer;
    dog: Dog | undefined;
}
```

Cloesce will automatically infer the relationship based on the property names (`dogId` and `dog`).

## One to Many
To define a One to Many relationship, we can use the `@OneToMany` decorator. Let's modify our models to allow a person to have multiple dogs:

```typescript
import { Model, Integer, ForeignKey, OneToMany } from "cloesce/backend";
@Model()
export class Dog {
    id: Integer;

    @ForeignKey(Person)
    ownerId: Integer;

    @OneToMany<Person>(d => d.ownerId)
    owner: Person | undefined;
}

@Model()
export class Person {
    id: Integer;

    @OneToMany<Dog>(d => d.ownerId)
    dogs: Dog[];
}
```

In this example, we added a foreign key `ownerId` to the `Dog` model, referencing the `Person` model. The `Person` model now has a navigation property `dogs`, which is an array of `Dog` instances, representing all dogs owned by that person.

We can omit decorators for `OneToMany` only if a single `ForeignKey` exists pointing from `Dog` to `Person`. Thus, the above code can be simplified to:

```typescript
import { Model, Integer } from "cloesce/backend";
@Model()
export class Dog {
    id: Integer;

    ownerId: Integer;
    owner: Person | undefined;
}

@Model()
export class Person {
    id: Integer;

    dogs: Dog[];
}
```

## Many To Many

Many to Many relationships have an intermediate junction table that holds foreign keys to both related models. Let's define a Many to Many relationship between `Student` and `Course` models:

```typescript
import { Model, Integer } from "cloesce/backend";
@Model()
export class Student {
    id: Integer;

    courses: Course[];
}

@Model()
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