# Navigation Fields

> [!NOTE]
> Navigation Fields only work in matching backings. For example, if you have a Durable Object backed Model and a D1 backed Model, you cannot create navigation fields that span across them.
>
> Any Model may navigate to a Worker backed Model, but Worker backed Models cannot navigate to other backings (D1 or Durable Objects).

Defining [foreign key relationships](./ch4-2-sqlite-constraints.md#foreign-key) between your Models sets SQL constraints to maintain data integrity, but it doesn't give you an easy way to access related data.

Navigation fields allow to set `1:1` and `1:M` relationships, hydrated by the [Cloesce ORM](./ch7-0-orm-reference.md)

## One-to-One Relationship

### Non SQLite Models

Given a relationship where a `Person` has one `Profile`, we can add a navigation field to the `Person` Model that allows us to access the related `Profile` directly:

```cloesce
model Profile {
    route {
        personId: string
    }

    // ... data
}

model Person {
    route {
        id: string
    }

    nav Profile::personId(id) {
        profile
    }
}
```

Here, the `nav Profile::personId(id)` block inside the `Person` Model tells Cloesce to create a navigation field called `profile` on the `Person` Model. 

When you query for a `Person`, Cloesce will automatically populate the `profile` property with the corresponding `Profile` instance based on the matching value of `Person`'s `id` and `Profile`'s `personId`.

> [!TIP]
> If `Profile` were to have no route fields, this syntax would be valid:
>
> ```cloesce
> model Person {
>   nav Profile { profile }
> }
> ```

### SQLite Models

> [!NOTE]
> Foreign key relationships and navigation fields are defined independently, but for SQLite backed models, navigation fields rely on the presence of a foreign key relationship to function. 
>
> Cloesce uses the foreign key relationship to determine how to populate the navigation field with the correct related data.

Given the same relationship where a `Person` has one `Profile`, but this time both are SQLite backed, we can define the navigation field like so:

```cloesce
model Profile for MyDb {
    primary {
        id: int
    }
}

model Person for MyDb {
    primary {
        id: int
    }

    foreign Profile::id {
        profileId
    }

    nav Profile::id(profileId) {
        profile
    }
}
```

In this example, `Person` has a foreign key relationship to `Profile` through the `profileId` field. The `nav Profile::id(profileId)` block inside the `Person` Model tells Cloesce to create a navigation field called `profile` on the `Person` Model.

When you query for a `Person`, Cloesce will automatically populate the `profile` property with the corresponding `Profile` instance based on the matching value of `Person`'s `profileId` and `Profile`'s `id`.

## One-to-Many Relationship

> [!NOTE]
> Only a SQLite backed Model can have a `1:M` relationship in the schema.

Let's say we want `Person` to have any number of `Dog`s. We can achieve this with a one-to-many relationship:

```cloesce
model Dog for Db {
    primary {
        id: int
    }

    foreign Person::id {
        ownerId
    }
}

model Person for Db {
    primary {
        id: int
    }

    nav Dog::ownerId {
        dogs // 1:M nav field!
    }
}
```

In this example, `Dog` has a foreign key relationship to `Person` through the `ownerId` field. On the `Person` Model, we declare a navigation field `dogs` that references the `Dog::ownerId` foreign key. Cloesce will populate the `dogs` property with an array of all `Dog` instances that have an `ownerId` matching the `Person`'s `id`.
