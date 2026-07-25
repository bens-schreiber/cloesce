# Custom Data Sources

Default capabilities for every Data Source are provided by Cloesce, but they can be naive:

- Children are not limited to a certain number of results, which can lead to overfetching.
- `many` relationships are ordered descending by primary key, which may not be the desired order.
- No filtering beyond what has been defined in the schema is provided.
- Everything is exposed to the client, which may not be desired for security reasons.

To combat this, Cloesce allows you to define custom Data Sources for any Model.

## Defining a Data Source

> [!NOTE]
> By default, any scalar property (i.e. SQLite columns) will be included by all Data Sources. They cannot be excluded.

Data Sources can be defined with a `source` block. In the inner `include` block, you can specify all fields to include in that Data Source, including R2, KV, and Navigation Fields.

```cloesce
source WithDogsOwnersDogs for Person {
    include {
        dogs {
            owner {
                dogs {
                    // ... could keep going!
                }
            }
        }
    }
}
```

### Overriding the Default Data Source

The Default Data Source for a Model can be overridden by giving a `source` block the name `Default`, changing the default behavior of that Model when it is hydrated without a specified Data Source:

```cloesce
// Override the default to be empty
source Default for Person {
    include {}

    // ...methods
}
```

## Get Method

Each time Cloesce needs to hydrate an instance of a D1 backed Model, it requires a Data Source with a `get` method defined.

If you do not define a `get` method, Cloesce will use a default `get-by-id` implementation. Otherwise, you can define a custom `get` method on any Data Source:

```cloesce
source ByName for Person {
    get {
        name: string
    }
}
```

A backend stub will be generated for the `get` method above, which you can then fill with custom logic for fetching a `Person` by their `name` instead of their `id`.

The `save` and `list` methods will use default implementations if not overriden.

### `instance` tag

Data Sources are used by API methods to denote how to hydrate an ["instance method"](TODO). From the clients perspective, an instance method is a method on a class, like:

```ts
const person = await Person.get({ id: 1 });
await person.instanceMethod();
```

How does Cloesce know which instance to call the method on? A naive Data Source could be defined like:

```cloesce
source ByName for Person {
    get {
        name: string
    }
}
```

which will result in the client expecting `name` to be passed on every instance method hydrated with `ByName`:

```ts
const person = await Person.get({ id: 1 });
await person.instanceMethod(person.name);
```

Because `person` already has the `name` field, it is redundant to require it to be passed in again. To tell Cloesce that a parameter is already available on the client instance (i.e. it is a field of the Model), you can use the `instance` tag in your `get` method parameters:

```cloesce
source ByName for Person {
    include {}

    get {
        [instance]
        name: string
    }
}
```

## List Method

The `get` method of a Data Source is special in that it can be used to hydrate an instance of a Model on an API call (see [instance methods](TODO)).

The `list` method however is purely utility for the backend and client to retrieve a list of instances of a Model.

```cloesce
source ByName for Person {
    include {
        dogs
        cats
        etc
    }

    list {
        lastSeenName: string
        limit: int
    }
}
```

## Internal Data Source

Data Sources are the preferred way to retrieve Models in Cloesce for both the backend and the client. However, you may not want to expose a Data Source to the client, and only use it internally in your backend.

Tag any Data Source with `internal` to prevent Cloesce from generating client methods for that Data Source:

```cloesce
[internal]
source InternalOnly for Person {
    // ...
}
```
