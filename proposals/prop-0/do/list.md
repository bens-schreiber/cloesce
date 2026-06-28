Given the Durable Object:

```cloesce
durable ShardedDo {
    shard {
        id: int
    }
}
```

# Same Database

In all of these examples, Models exist on the same Durable Object and on the same shard (same shard id, doId = doId).

## One-to-One Relationships

```cloesce
model ModelA for ShardedDo(doId) {
    primary {
        id: int
    }

    foreign ModelB::id {
        modelBId
    }

    one ModelB::{doId(doId), id(modelBId)} {
        modelB
    }
}

model ModelB for ShardedDo(doId) {
    primary {
        id: int
    }
}
```

EX: `LIST ModelA`

```json
[
  [
    {
      "db": {
        "name": "ShardedDo",
        "args": {
          "from_params": ["doId"]
        }
      },
      "query": {
        "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1 JOIN ModelB ON ModelA.modelBId = ModelB.id",
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": ""
    }
  ]
]
```

## One-to-Many Relationships

```cloesce
model ModelA for ShardedDo(doId) {
    primary {
        id: int
    }

    many ModelB::{doId(doId), modelAId(id)} {
        modelBs
    }
}

model ModelB for ShardedDo(doId) {
    primary {
        id: int
    }
}
```

EX: `LIST ModelA`

```json
[
  [
    {
      "db": {
        "name": "ShardedDo",
        "args": {
          "from_params": ["doId"]
        }
      },
      "query": {
        "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1 JOIN ModelB ON ModelA.id = ModelB.modelAId",
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": ""
    }
  ]
]
```

# Different Databases

## One-to-One Relationships

```cloesce
model ModelA for ShardedDo(doId) {
    primary {
        id: int
    }

    column {
        modelBDoId: int
        modelBId: int
    }

    one ModelB::{doId(modelBDoId), id(modelBId)} {
        modelB
    }
}

model ModelB for ShardedDo(doId) {
    primary {
        id: int
    }
}
```

EX: `LIST ModelA`

```json
[
  [
    {
      "db": {
        "name": "ShardedDo",
        "args": {
          "from_params": ["doId"]
        }
      },
      "query": {
        "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1",
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": ""
    }
  ],
  [
    {
      "db": {
        "name": "ShardedDo",
        "args": {
          "from_result": ["modelBDoId"]
        }
      },
      "query": {
        "sql": "SELECT * FROM ModelB WHERE ModelB.id = ?1",
        "args": {
          "from_result": ["modelBId"]
        },
        "map": "one"
      },
      "result": "modelB"
    }
  ]
]
```

## One-to-Many Relationships

```cloesce
model ModelA for ShardedDo(doId) {
    primary {
        id: int
    }

    column {
        modelBDoId: int
    }

    many ModelB::{doId(modelBDoId), modelAId(id)} {
        modelBs
    }
}

model ModelB for ShardedDo(doId) {
    primary {
        id: int
    }

    column {
        modelAId: int
    }
}
```

EX: `LIST ModelA`

```json
[
  [
    {
      "db": {
        "name": "ShardedDo",
        "args": {
          "from_params": ["doId"]
        }
      },
      "query": {
        "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1",
        "args": {
          "from_params": ["id"]
        },
        "map": "many"
      },
      "result": ""
    }
  ],
  [
    {
      "db": {
        "name": "ShardedDo",
        "args": {
          "from_result": ["modelBDoId"]
        }
      },
      "query": {
        "sql": "SELECT * FROM ModelB WHERE ModelB.modelAId = ?1",
        "args": {
          "from_result": ["id"]
        },
        "map": "many"
      },
      "result": "modelBs"
    }
  ]
]
```
