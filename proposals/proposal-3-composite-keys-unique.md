# Proposal: Composite Keys, Unique Constraints, Fluent API

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-02-26
- **Last Updated:** 2026-02-26

---

## Summary

This proposal aims to add support for composite keys and unique constraints in the D1 database schema. This will allow developers to define multiple columns as a primary key, as well as enforce uniqueness in any combination of columns.

---

## Motivation

Cloesce has no way to define composite keys or unique constraints, which limits the ability to model common relationships and enforce data integrity.

An example use case is found in an [early demo of Cloesce](https://github.com/bens-schreiber/cloesce-004-demo/blob/df8d922a22dcdea2d6a5d9a5b41554ae4dfb985e/src/backend/models.cloesce.ts#L156), which notes that a query is inefficient due to the inability to define composite keys.

The situation is modeled as follows:

```ts

@Model()
export class Course {
  id: Integer;
  // ... 
}

@Model()
export class Professor {
  id: Integer;

  // ...
  courseRatings: ProfessorCourseRating[];
}

@Model()
export class ProfessorCourseRating {
    id: Integer;

    @ForeignKey(Professor)
    professorId: Integer;

    @ForeignKey(Course)
    courseId: Integer;
}
```

In this example, a Professor can have multiple ratings for different courses, but the combination of `professorId` and `courseId` should be unique to prevent duplicate ratings for the same course by the same professor. The course rating in this case isn't individual ratings from students, but rather an overall rating for the course by the professor, which is why it makes sense to enforce uniqueness on the combination of `professorId` and `courseId`.

Two approaches to modeling this are:
1. **Using a surrogate primary key**: This is the current approach, where `id` is the primary key, and we would need to add a unique constraint on the combination of `professorId` and `courseId`.
2. **Using a composite primary key**: This would allow us to define `professorId` and `courseId` together as the primary key, eliminating the need for a surrogate key and ensuring uniqueness by design.

The first approach is do-able in Cloesce, by manually modifying the generated SQL schema to add a unique constraint (a method that shouldn't frown upon, as Cloesces generated SQL schema should be considered a starting point for further customization). However, it is not ideal as it requires manual intervention and is not supported by the D1 schema definition language.

The second approach is completely unsupported in Cloesce, and requires a significant change to the way primary keys are defined and handled in the D1 schema.
---

## Goals and Non-Goals

### Goals

- Allow developers to define composite primary keys in the D1 schema.
- Allow developers to define unique constraints on any combination of columns in the D1 schema.
- Querying of composite primary key columns in the ORM.
- Composite foreign keys that reference composite primary keys.


### Non-Goals

- Indexes. Primary keys and unique constraints use indexes under the hood, but this proposal does not aim to expose a way to write indexes in Cloesce.

---

## Detailed Design

### Cloesce Configuration (Fluent API)

In many cases, it is difficult or unwieldy to express certain relationships and constraints through decorators on the Model class alone. These relationships do change the structure of the schema, but rather "SQL metadata".

Instead of creating an overbearing verbose syntax on top of a Model class, we will introduce a new `cloesce.config.ts` file that can be used to programatically modify the extracted AST before it is handed off to the generator. The Entity Framework inspired Fluent API can define all things the current decorator syntax can, as well as the new features proposed in this document.

```ts
// cloesce.config.ts
import { defineConfig } from "cloesce";

const config: CloesceConfig = defineConfig({
    srcPaths: [
        "./src/data"
    ],
    workersUrl: "http://localhost:5000/api",
    migrationsPath: "./migrations"
});

defineConfig.modelBuilder(Foo, (builder) => {
    // 1. Define a primary key
    builder.primaryKey("id");

    // 2. Define a foreign key
    builder.foreignKey("barId").references(Bar, "id");

    // 3. Define a one to one relationship
    builder.oneToOne("bar").references(Bar, "id");

    // 4. Define a one to many relationship
    builder.oneToMany("bars").references(Bar, "fooId");

    // 5. Define a many to many relationship
    builder.manyToMany("bars").references(Bar, "id");

    // NOTE: KV and R2 to come in the future? May not be necessary.
});

defineConfig.rawAst(ast => {
    // This function will be given the raw AST extracted from the source code and can modify it 
    // arbitrarily before it is passed to the generator.
    //
    // May void your warranty.
});

export default config;
```

### Unique Constraints

To define a unique constraint on some combination of columns, the `modelBuilder` function in `cloesce.config.ts` can be used to programmatically add unique constraints to the Model. This allows for a more flexible and powerful way to define constraints without cluttering the Model class with too many decorators.

```ts
defineConfig.modelBuilder(ProfessorCourseRating, (builder) => {
    builder.unique("courseId", "professorId");
    builder.unique("name");
});
```

This syntax defines a unique constraint for `(professorId, courseId)` and `(name)`, meaning that the combination of `professorId` and `courseId` must be unique, and the `name` must also be unique on its own.

```sql
CREATE TABLE ProfessorCourseRating (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE,
    professorId INTEGER,
    courseId INTEGER,
    FOREIGN KEY (professorId) REFERENCES Professor(id),
    FOREIGN KEY (courseId) REFERENCES Course(id),
    UNIQUE (professorId, courseId)
```

### Composite Primary Keys

A primary key may be made up of multiple columns. Those columns together may be foreign keys to other tables. A simple way to express this is to allow multiple `@PrimaryKey` decorators on the properties that should be part of the primary key. The order of the decorators will determine the order of the columns in the primary key definition. It will be allowed to stack `@PrimaryKey` with `@ForeignKey`.

```ts
@Model()
export class ProfessorCourseRating {
    @PrimaryKey()
    @ForeignKey(Professor)
    professorId: Integer;

    @PrimaryKey()
    @ForeignKey(Course)
    courseId: Integer;

    name: String;
}
```

This would translate to the following SQL schema:

```sql
CREATE TABLE ProfessorCourseRating (
    professorId INTEGER,
    courseId INTEGER,
    name TEXT,
    PRIMARY KEY (professorId, courseId),
    FOREIGN KEY (professorId) REFERENCES Professor(id),
    FOREIGN KEY (courseId) REFERENCES Course(id)
);
```

Optionally, this could be expressed through the `modelBuilder` function in `cloesce.config.ts` instead of using decorators:

```ts
defineConfig.modelBuilder(ProfessorCourseRating, (builder) => {
    builder.primaryKey("professorId", "courseId");
    builder.foreignKey("professorId").references(Professor, "id");
    builder.foreignKey("courseId").references(Course, "id");
});
```

### Composite Foreign Keys

If a Model can have a composite primary key, it stands to reason that we should also be able to reference that composite primary key with a composite foreign key. This will be done through the `modelBuilder` function in `cloesce.config.ts`, which will allow us to define a composite foreign key that references a composite primary key.

```ts
@Model()
class SomeModel {
    id: Integer;

    professorId: Integer;
    courseId: Integer;
    professorCourseRating: ProfessorCourseRating;
}
```

```ts
defineConfig.modelBuilder(SomeModel, (builder) => {
    builder.foreignKey("professorId", "courseId")
        .references(ProfessorCourseRating, "professorId", "courseId");

    builder.oneToOne("professorCourseRating")
        .references(ProfessorCourseRating, "professorId", "courseId");
});
```

This would translate to the following SQL schema:

```sql
CREATE TABLE SomeModel (
    id INTEGER PRIMARY KEY,
    professorId INTEGER,
    courseId INTEGER,
    FOREIGN KEY (professorId, courseId) REFERENCES ProfessorCourseRating(professorId, courseId)
);
```

### Many to Many with Composite Keys

Cloesce supports many to many relationships through the use of join tables, such as:
```ts
@Model()
class Student {
    id: Integer;
    name: String;

    courses: Course[];
}

@Model()
class Course {
    id: Integer;
    name: String;

    students: Student[];
}

// => Implicit join table with composite primary key (courseId, studentId)
```

What if one of these models has a composite primary key? For example, if `Course` had a composite primary key of `(id, name)`, how would we define the many to many relationship between `Student` and `Course`?
The `modelBuilder` function in `cloesce.config.ts` can be used to define the many to many relationship with composite keys:

```ts
@Model()
class Course {
    @PrimaryKey()
    id: Integer;

    @PrimaryKey()
    name: String;

    students: Student[];
}

@Model()
class Student {
    id: Integer;
    name: String;

    courses: Course[];
}
```

```ts
defineConfig.modelBuilder(Student, (builder) => {
    builder.manyToMany("courses").references(Course, "id", "name");
});
```

This would translate to the following SQL schema:

```sql
CREATE TABLE Course (
    id INTEGER,
    name TEXT,
    PRIMARY KEY (id, name)
);
CREATE TABLE Student (
    id INTEGER PRIMARY KEY,
    name TEXT
);

CREATE TABLE CourseStudent (
    courseId INTEGER,
    courseName TEXT,
    studentId INTEGER,
    PRIMARY KEY (courseId, courseName, studentId),
    FOREIGN KEY (courseId, courseName) REFERENCES Course(id, name),
    FOREIGN KEY (studentId) REFERENCES Student(id)
);
```


## Implementation

### Cloesce Configuration (Fluent API)

The `cloesce.config.ts` file will be implemented as a new entry point for the Cloesce configuration. It will export a `defineConfig` function that can be used to define the configuration for the Cloesce generator, including the new `modelBuilder` function for defining composite keys and unique constraints.

When extraction finishes, the `modelBuilder` functions defined in `cloesce.config.ts` will be called with the extracted AST for each Model, allowing the developer to programmatically modify the AST to add composite keys, unique constraints, and other metadata before it is passed to the generator.

Finally, the `rawAst` function will be called with the entire extracted AST with applied metadata, allowing for arbitrary modifications to the AST before it is passed to the generator.

### Unique Constraints

A new property `unique_constraints` will be added to the Model AST, consisting of an array of unique constraint definitions. The generator will then translate these unique constraint definitions into the appropriate SQL syntax when generating the schema.

A single unique constraint can be added inline to the table definition, while multiple unique constraints will be added as separate `UNIQUE` clauses.

The migrations engine must be capable of creating tables with unique constraints, as well as removing and adding a unique constraint. All of these will require a full table build, as SQLite does not support adding or removing unique constraints through `ALTER TABLE`. To account for this, `unique_constraints` will be added to the Merkle hash calculation for the Model, so that any changes to unique constraints will trigger a full table rebuild.

A Rust struct for the unique constraint definition may look like:

```rust
struct UniqueConstraint {
    columns: Vec<Vec<String>>, // e.g., [["professorId", "courseId"], ["name"]]
    hash: u64, // merkle hash
}
```

Overall, this change is additive to the migrations engine portion of the generator.

### Composite Keys

Primary keys are currently defined outside of the `columns` property of the Model AST, as a single `primary_key` property. Further, primary keys can also have foreign key references. The best way to support this move is going to be to move the indicator of a primary key to a boolean property on each column definition.

Additionally, a column may be apart of a composite key. A field `composite_key_id` can be added to the column definition, which will be an optional string. If it is `None`, then the column is not part of a composite key. If it is `Some(id)`, then the column is part of the composite key with the given id. The order of the columns in the composite key can be determined by the order of the columns in the Model definition.

```rust
pub struct D1Column {
    #[serde(default)]
    pub hash: u64,

    /// Symbol name and Cloesce type of the attribute.
    /// Represents both the column name and type.
    pub value: NamedTypedValue,

    /// If the attribute is a foreign key, the referenced model name.
    /// Otherwise, None.
    pub foreign_key_reference: Option<String>,

    /// The ID of the composite key this column belongs to, if any.
    pub composite_key_id: Option<String>,

    /// If the attribute is a primary key, this will be true.
    /// Otherwise, false.
    pub is_primary_key: bool,
}

// ...
impl Model {

    /// Returns the indices of the columns that are part of the primary key.
    pub fn primary_key(&self) -> Vec<usize> { ... } 

    /// Returns a vector of composite keys, where each composite key is a vector of column indices.
    pub fn composite_keys(&self) -> Vec<Vec<usize>> { ... }
}
```

This change will have significant repercussions throughout the entire codebase, as the concept of a primary key is currently deeply ingrained in the way Models are defined and handled.

