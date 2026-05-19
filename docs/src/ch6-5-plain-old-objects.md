# Plain Old Objects

In addition to [Models](./ch4-0-models.md), Cloesce also supports Plain Old Objects (POOs) that can be used for various purposes such as data transfer objects (DTOs), view models, or any other structured data that doesn't require database backing. POOs are defined using the `poo` keyword and can have fields just like Models, but they don't have any of the [ORM](./ch7-0-orm-reference.md) or [API](./ch6-1-rest-apis.md) capabilities that Models have.

## Defining a POO

To define a POO, you can use the following syntax:

```cloesce
poo PersonDTO {
    id: int
    name: string
    age: int
}
```

The above code defines a POO called `PersonDTO` with three fields: `id`, `name`, and `age`. You can use this POO in your API definitions, data sources, or anywhere else you need to represent structured data without the overhead of a full Model.

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
