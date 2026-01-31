# Include Trees

In the previous section, we discussed how to define navigation properties in our Models to represent relationships between entities. However, if you try to fetch a Model instance with navigation properties, you will notice that the navigation properties are not populated by default. This is where include trees come into play.

## What are Include Trees?

Include Trees are Cloesce's response to the overfetching and recursive relationship challenges faced in data retrieval. For example, in the Model definition below how should Cloesce know how deep to go when fetching a `Person` and their associated `Dog`?

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

If we were to follow this structure naively, fetching a `Person` would lead to fetching their `Dog`, which would lead to fetching the `Person` again, and so on, resulting in infinite recursion.

Include Trees allow developers to explicitly specify which related entities should be included in the query results, preventing overfetching and controlling the depth of data retrieval.

By default, all scalar properties (e.g., `string`, `number`, `boolean`, etc.) are always included in query results. Include Trees are only necessary for navigation properties.

It is common to type navigation properties as possibly `undefined` to indicate that they may not be populated unless explicitly included.

> *Alpha Note*: No default include behavior is implemented yet. All navigation properties must be explicitly included using Include Trees.

## Using Include Trees

To solve the problem presented above, we can use Include Trees to specify that when fetching a `Person`, we want to include their `dogs`, but not the `owner` property of each `Dog`. Here's how we can do that:

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

During Cloesce's extraction phase, the compiler recognizes the `IncludeTree` type and processes the structure accordingly. Client code generation will then have the option to use this Include Tree when querying for `Person` instances.

Include Trees are not limited to only D1 Models; they can be used with KV and R2 as well.