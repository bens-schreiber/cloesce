# v0.0.5 Syntax Sugaring

Defining Models, Services and interacting with the `CloesceApp` currently has the weight of boilerplate through decorators and explicit registration. In v0.0.5, we will introduce syntax sugaring to make this process more ergonomic.

## Entity Framework Inspired Sugaring

### Primary Key
Instead of requiring `@PrimaryKey` to be added to a Model, we can assume any field named `id` or `<model_name>id` is the primary key. To allow all kinds of casing, names will be normalized to lowercase and stripped of underscores, so `user_id`, `UserID`, and `ID` will all be valid primary key names for the `User` model.

```ts
@Model
export class User {
    id: Integer; // => PrimaryKey
}
```

Of course, if a different field is desired as the primary key, the `@PrimaryKey` decorator can still be used to override the default behavior. This will be useful when composite primary keys are added in a future release.

### One to One

To make a One to One relationship, the current pattern is:
```ts
@Model
class User {
    id: Integer;
}

@Model
class Dog {
    id: Integer;

    @ForeignKey(User)
    ownerId: Integer;

    @OneToOne("ownerId")
    owner: User;
}
```

Entity Framework infers One to One relationships based on the presence of a foreign key field and the corresponding navigation property. Thus, the above can be simplified to. Essentially, we can look for a field named `<navigation_property_name>Id` or `<related_model_name>Id` (normalized like in primary keys to allow all casing) to infer the foreign key relationship.
```ts
@Model
class User {
    id: Integer;
}

@Model
class Dog {
    id: Integer;

    ownerId: Integer;
    owner: User;
}
```

### One to Many
Similar to One to One relationships, One to Many relationships can be inferred by the presence of a foreign key field in the "many" side of the relationship. Thus, the current pattern:
```ts
@Model
class User {
    id: Integer;

    @OneToMany("ownerId")
    dogs: Dog[];
}

@Model
class Dog {
    id: Integer;

    @ForeignKey(User)
    ownerId: Integer;
}
```

Can be simplified to:
```ts
@Model
class User {
    id: Integer;
    dogs: Dog[];
}

@Model
class Dog {
    id: Integer;
    ownerId: Integer;
}
```

Note that the foreign key field `ownerId` is still required in the `Dog` model to establish the relationship. We won't implement shadow keys like in Entity Framework, where the foreign key field is automatically created if it doesn't exist.

Also worth noting is that the One to Many relationship is inferred from the presence of the navigation property `dogs` in the `User` model, and the foreign key field `ownerId` in the `Dog` model. This inference breaks if there are multiple relationships between the same two models, in which case explicit decorators will be required to disambiguate.

### Many to Many

Many to Many relationships can also be inferred by the presence of navigation properties on both sides of the relationship. Thus, the current pattern:
```ts
@Model
class Student {
    id: Integer;

    @ManyToMany()
    courses: Course[];
}

@Model
class Course {
    id: Integer;

    @ManyToMany()
    students: Student[];
}
```

Can be simplified to:
```ts
@Model
class Student {
    id: Integer;
    courses: Course[];
}

@Model
class Course {
    id: Integer;
    students: Student[];
}
```

The Many to Many decorator will be dropped entirely, and the relationship will be inferred from the presence of the navigation properties on both sides.


## Refactors

### Plain Old Objects

The current pattern to register a `PlainOldObject` is to mark a class with the `@PlainOldObject` decorator:
```ts
@PlainOldObject
export class SomePoo {
    name: string;
    age: number;
}
```

Marking every POO with a decorator is tedious and unnecessary. We can remove the decorator completely, and use context clues to determine what should be a POO. Specifically, any class that is used in a method as a parameter or return type that is not a Model or scalar type will be treated as a Plain Old Object, and automatically registered as such. All plain old objects will have to be marked with `export` still.

```ts
// By itself, not compiled to the CIDL
export class SomePoo {
    name: string;
    age: number;
}

@Model
export class User {
    // ...


    @POST
    async getSomePoo(): SomePoo {
        // `SomePoo` is referenced, and the extractor will validate and register it as a POO
        // the first time it is encountered.
        return { name: "Alice", age: 30 };
    }

}
```

### CRUD Operations

Defining CRUD operations for Models introduces a secondary decorator `CRUD` which can be merged with the `Model` decorator, revealing:
```ts
@Model(["GET", "SAVE"])
export class User {...}
```

### Data Sources

Data sources are explicitly marked with `@DataSource`, but this can be removed because all Data Sources are of type `IncludeTree<T>`.


