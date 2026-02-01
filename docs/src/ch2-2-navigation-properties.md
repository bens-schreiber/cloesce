# Navigation Properties and Include Trees

In the previous section, we created a basic D1 Model with scalar properties. However, relational databases like Cloudflare D1 typically more complex relationships between tables. 

In this section, we will explore *Navigation Properties* and *Include Trees*, which allow us to define and hydrate relationships between different Models.

## Foreign Keys

Before diving into navigation properties, it's essential to understand foreign keys. Foreign keys are scalar properties that reference the primary key of another Model, establishing a relationship between two Models.

Foreign keys directly translate to SQLite `FOREIGN KEY` constraints in the underlying D1 database.

For example, let's say we want to create a relationship between `Person` and `Dog`, where a Person can have one Dog

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

In the above code, we defined a `Dog` Model and a `Person` Model. 

The `Person` Model has a foreign key property `dogId`, which references the primary key of the `Dog` Model. This establishes a relationship where each person can be associated with one dog.

> *NOTE*: Cloesce does not allow circular foreign key relationships (and neither does SQLite!). 
>
> If you need to model such a relationship, consider marking a foreign key as nullable and managing the relationship at the application level.

## Navigation Properties

Inspired by [Entity Framework relationship navigations](https://learn.microsoft.com/en-us/ef/core/Modeling/relationships/navigations), Cloesce allows you to effortlessly define One to One, One to Many and Many to Many relationships between your Models using Navigation Properties. All navigation properties are D1 backed Models themselves, or arrays of D1 backed Models.

Let's revisit our `Person` and `Dog` Models and add navigation properties to them:

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

In this example, we added a navigation property `dog` to the `Person` Model using the `@OneToOne` decorator. 

This property allows us to access the associated `Dog` instance directly from a `Person` instance. The type of the navigation property is `Dog | undefined`, indicating that it may or may not be populated (elaborated on  in the [Include Trees](./ch2-3-include-trees.md) section).

Just like in Entity Framework, omitting decorators is possible when specific naming conventions are followed. The above code can be reduced to:

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
To define a One to Many relationship, we can use the `@OneToMany` decorator. Let's modify our Models to allow a Person to have multiple Dogs:

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

In this example, we added a foreign key `ownerId` to the `Dog` Model, referencing the `Person` Model. The `Person` Model now has a navigation property `dogs`, which is an array of `Dog` instances, representing all dogs owned by that person.

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

Many to Many relationships have an intermediate junction table that holds foreign keys to both related Models. Let's define a Many to Many relationship between `Student` and `Course` Models:

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

> *Note*: The left column lists the model name that comes first alphabetically; the right column lists the one that comes after.