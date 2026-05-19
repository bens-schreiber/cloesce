# D1 Navigation Fields

> [!NOTE]
> Navigation Field are (currently) constrained to work within one particular database. This means that if you have two separate D1 databases, you cannot define a navigation field that references a foreign key in the other database.

Simply defining foreign key relationships between your Models is often not enough. You also want to be able to easily access related data without having to write complex `JOIN` queries. This is where Navigation Fields come in.

## One-to-One Relationship

Given a relationship where `Person` has one `Dog`, we can add a navigation field to the `Person` Model that allows us to access the related `Dog` directly:

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

    foreign (Dog::id) {
        dogId
        nav { dog } // nav field!
    }
}
```

In this example, the `nav { dog }` block inside the foreign key declaration tells Cloesce to create a navigation field called `dog` on the `Person` Model. When you query for a `Person`, Cloesce will automatically populate the `dog` property with the corresponding `Dog` instance based on the foreign key relationship.

All one-to-one navigation fields exist within a foreign key block, and are populated based on the foreign key relationship defined in that block.

### Transpiled Code

While one-to-one navigation fields do not directly translate to the SQL schema, they do generate additional code in both the frontend and backend layers of your application to enable this functionality.

```ts
// .cloesce/client.ts
export class Person {
    id: number;
    dogId: number;
    dog: Dog; // navigation field
}
```

```ts
// .cloesce/backend.ts
export namespace Person {
    // ...
    export interface Self {
        id: number;
        dogId: number;
        dog: Dog.Self; // navigation field
    }
}
```

## One-to-Many Relationship

Let's now say we want `Person` to have any number of `Dogs`. We can achieve this with a one-to-many relationship:

```cloesce
model Dog {
    primary {
        id: int
    }

    foreign (Person::id) {
        ownerId
    }
}

model Person {
    primary {
        id: int
    }

    nav (Dog::ownerId) {
        dogs // nav field!
    }
}
```

In this example, `Dog` has a foreign key relationship to `Person` through the `ownerId` field. On the `Person` Model, we declare a navigation field `dogs` that references the `Dog::ownerId` foreign key. Cloesce will populate the `dogs` property with an array of all `Dog` instances that have an `ownerId` matching the `Person`'s `id`.

### Transpiled Code

```ts
// .cloesce/client.ts
export class Person {
    id: number;
    dogs: Dog[]; // navigation field
}
```

```ts
// .cloesce/backend.ts
export namespace Person {
    // ...
    export interface Self {
        id: number;
        dogs: Dog.Self[]; // navigation field
    }
}
```

## Many-to-Many Relationship

Many-to-many relationships are achieved in SQLite through creating a join table with a composite primary key. While you can define this join table as its own Model in Cloesce (and you may need to if you want to store additional data on the relationship), Cloesce also provides a convenient way to define many-to-many relationships with navigation fields.

Consider the relationship where `Student` has many `Courses`, and `Course` has many `Students`:

```cloesce
model Course {
    primary {
        id: int
    }

    nav (Student::courses) {
        students
    }
}

model Student {
    primary {
        id: int
    }

    nav (Course::students) {
        courses
    }
}
```

In this example, we declare a navigation field `students` on the `Course` Model that references the `Student::courses` navigation field, and vice versa. Cloesce will automatically create the necessary join table and populate the navigation fields with the related data.

### Transpiled Code

> [!NOTE]
> The left column lists the Model name that comes first alphabetically; the right column lists the one that comes after.

```sql
CREATE TABLE IF NOT EXISTS "CourseStudent" (
  "left" integer NOT NULL,
  "right" integer NOT NULL,
  PRIMARY KEY ("left", "right"),
  FOREIGN KEY ("left") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("right") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);
```