# Include Trees

If you try to fetch a Model instance that has Navigation Properties or KV and R2 attributes, you will notice that they are not populated by default (they will be either empty arrays or undefined). This is intentional and is handled by Include Trees.

## What are Include Trees?

Include Trees are Cloesce's response to the overfetching and recursive relationship challenges when modeling a relational database with object-oriented paradigms.

For example, in the Model definition below, how should Cloesce know how deep to go when fetching a Person and their associated Dog?

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

// => { id: 1, dogs: [ { id: 1, owner: { id: 1, dogs: [ ... ] } } ] } ad infinitum
```

If we were to follow this structure naively, fetching a `Person` would lead to fetching their `Dog`, which would lead to fetching the same `Person` again, and so on, infinitely.

Include Trees allow developers to explicitly state which related Navigation Properties should be included in the query results, preventing overfetching. If a Navigation Property is not included in the Include Tree, it will remain `undefined` (for singular relationships) or an empty array (for collections).

A common convention to follow when writing singular Navigation Properties is to define them as `Type | undefined`, indicating that they may not be populated unless explicitly included.

> All scalar properties (e.g., `string`, `number`, `boolean`, etc.) are always included in query results. Include Trees are only necessary for Navigation Properties.

> *Alpha Note*: No default include behavior is implemented yet. All Navigation Properties must be explicitly included using Include Trees.

## Creating an Include Tree

We can define Include Trees to specify that when fetching a `Person`, we want to include their `dogs`, but not the `owner` property of each `Dog`:

```typescript
import { Model, Integer, IncludeTree } from "cloesce/backend";
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

    static readonly withDogs: IncludeTree<Person> = {
        dogs: {
            owner: {
                // Left empty to signal no more includes
                // ...but we could keep going!
                // dogs: { ... }
            }
        }
    };
}
```

In this example, we defined a static property `withDogs` on the `Person` Model that represents an Include Tree. This tree specifies that when fetching a `Person`, we want to include their `dogs`, but we do not want to include the `owner` property of each `Dog`.

During Cloesce's extraction phase, the compiler recognizes the `IncludeTree` type and processes the structure accordingly. 

Client code generation will then have the option to use this Include Tree when querying for `Person` instances. See the [Cloesce ORM](./ch2-6-cloesce-orm.md) chapter and [Model Methods](./ch2-5-Model-methods.md) for more details on how to use Include Trees in queries.

> Include Trees are not limited to only D1 backed Models; they can be used with KV and R2 as well.