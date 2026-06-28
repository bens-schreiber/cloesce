With the Cloesce Query Planner, Worker based relationships will be enabled with both `one` and `many` syntaxes. Because a Worker based Model cannot be truly "listed", the `many` syntax will simply map to an array with at most one element.

Additionally, the `list` method for Worker based models will return a single element array containing the Worker based model, or an empty array if the Worker based model does not exist. This allows for consistent handling of Worker based relationships in queries, while still adhering to the limitations of Worker based models.

# `get` Query

## One-to-One Relationships

```cloesce
model User {
    route {
        id: int
    }

    one Cat::id(id) {
        cat
    }
}

model Cat {
    route {
        userId: int
    }
}
```

=>

```json
[
  [
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "one"
      },
      "result": ""
    },
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "one"
      },
      "result": "cat"
    }
  ]
]
```

## One-to-Many Relationships

```cloesce
model User {
    route {
        id: int
    }

    many Cat::userId(id) {
        cats
    }
}

model Cat {
    route {
        userId: int
    }
}
```

```json
[
  [
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "one"
      },
      "result": ""
    },
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": "cats"
    }
  ]
]
```

Even though `cats` is `many`, the runtime will only return an array with at most one element, since Worker based models cannot be truly "listed".

# `list` Query

## One-to-One Relationships

```cloesce
model User {
    route {
        id: int
    }

    one Cat::id(id) {
        cat
    }
}

model Cat {
    route {
        userId: int
    }
}
```

```json
[
  [
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": ""
    },
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "one"
      },
      "result": "cat"
    }
  ]
]
```

The `list` method must return an array, so the runtime will map a single value to an array with at most one element. If the Worker based model does not exist, the runtime will return an empty array. Additionally, `list` will require all route parameters to be supplied, since the runtime cannot infer any route parameters from the `list` method.

## One-to-Many Relationships

```cloesce
model User {
    route {
        id: int
    }

    many Cat::userId(id) {
        cats
    }
}

model Cat {
    route {
        userId: int
    }
}
```

```json
[
  [
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": ""
    },
    {
      "query": {
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": "cats"
    }
  ]
]
```

# Parameter-less Relationships

A Worker based model does not require any route fields:

```cloesce
model User {
    one Cat {
        cat
    }

    many Dog {
        dogs
    }
}

model Cat {}

model Dog {}
```

EX: `GET User`

```json
[
  [
    {
      "query": {
        "map": "one"
      },
      "result": ""
    },
    {
      "query": {
        "map": "one"
      },
      "result": "cat"
    },
    {
      "query": {
        "map": "many"
      },
      "result": "dogs"
    }
  ]
]
```
