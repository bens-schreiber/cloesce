# Plain Old Objects

In addition to [Models](./ch4-0-models.md), Cloesce supports Plain Old Objects (POOs) for structured data that doesn't require database backing, such as data transfer objects (DTOs) or view models.

POOs are defined with the `poo` keyword and can have fields just like Models, but they lack the [ORM](./ch7-0-orm-reference.md) and [API](./ch6-1-rest-apis.md) capabilities that Models have.

## Defining a POO

To define a POO, you can use the following syntax:

```cloesce
poo PersonDto {
    id: int
    name: string
    age: int
}
```

The above code defines a POO called `PersonDto` with three fields: `id`, `name`, and `age`. You can use this POO in your API definitions, data sources, or anywhere else you need to represent structured data without the overhead of a full Model.

## POO Composition

POOs can also be composed of other POOs, allowing you to create complex data structures. For example:

```cloesce
poo GraphNode {
    id: int
    value: string
    children: array<GraphNode>
}
```

In the above code, the `GraphNode` POO has a field `children` which is an array of `GraphNode`s, allowing you to represent tree-like structures.

## `[internal]` POOs

A Plain Old Object can be marked as `[internal]`, which prevents any API method from accepting or returning that POO. For example:

```cloesce
[internal]
poo UserCredentials {
    password: string
}

kv Credentials {
    creds -> UserCredentials {
        username: string
    }
}
```

Here, `UserCredentials` is internal, so no API method can accept or return it. It can still be used in a KV template, since KV templates are not exposed to the client.

A Model may still have a `Credentials::creds` KV field, but that field cannot be exposed through an API method, since `UserCredentials` itself is internal.
