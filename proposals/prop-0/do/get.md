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

EX: `GET ModelA`

```json
[
    [
        {
            "db": {
                "name": "ShardedDo",
                "args": {
                    "from_params": ["doId"]
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1 JOIN ModelB ON ModelA.modelBId = ModelB.id",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one",
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

EX: `GET ModelA`

```json
[
    [
        {
            "db": {
                "name": "ShardedDo",
                "args": {
                    "from_params": ["doId"]
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1 JOIN ModelB ON ModelA.id = ModelB.modelAId",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one",
            },
            "result": ""
        }
    ]
]
```

# Different Database

Assume in all of these cases that the Models exist on different shards (doId != doId).

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

EX: `GET ModelA`

```json
[
    [
        {
            "db": {
                "name": "ShardedDo",
                "args": {
                    "from_params": ["doId"]
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one",
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
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelB WHERE ModelB.id = ?1",
                "args": {
                    "from_result": ["modelBId"]
                },
                "map": "one",
            },
            "result": "modelB"
        }
    ]
]
```

Because `doId` _may not_ equal `modelBDoId`, the Query Planner will generate two separate queries to be executed on two different Durable Objects. The first query will be executed on the DO that contains `ModelA`, and the second query will be executed on the DO that contains `ModelB`. The runtime will execute the first query, and then use the results of that query to execute the second query.

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

EX: `GET ModelA`

```json
[
    [
        {
            "db": {
                "name": "ShardedDo",
                "args": {
                    "from_params": ["doId"]
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one",
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
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelB WHERE ModelB.modelAId = ?1",
                "args": {
                    "from_result": ["id"]
                },
                "map": "many",
            },
            "result": "modelBs"
        }
    ]
]
```

# Hybrid

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

    many ModelC::{doId(doId), modelAId(id)} {
        modelCs
    }
}

model ModelB for ShardedDo(doId) {
    primary {
        id: int
    }
}

model ModelC for ShardedDo(doId) {
    primary {
        id: int
    }

    foreign ModelA::id {
        modelAId
    }
}
```

In this example, `ModelA` has a `one` relationship with `ModelB` through a variable `modelBDoId`. Additionally, `ModelA` has a `many` relationship with `ModelC` through the same shard id, meaning it exists on the same Durable Object shard. A query with two transactions will be generated:

```json
[
    [
        {
            "db": {
                "name": "ShardedDo",
                "args": {
                    "from_params": ["doId"]
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?1 JOIN ModelC ON ModelA.id = ModelC.modelAId",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one",
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
                },
            },
            "query": {
                "sql": "SELECT * FROM ModelB WHERE ModelB.id = ?1",
                "args": {
                    "from_result": ["modelBId"]
                },
                "map": "one",
            },
            "result": "modelB"
        }
    ]
]
```

