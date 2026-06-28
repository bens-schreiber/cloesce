# Same Database

## One-to-One Relationships

```cloesce
model ModelA for DbA {
    primary {
        id: int
    }

    foreign ModelB::id {
        modelBId
    }

    one ModelB::id(modelBId) {
        modelB
    }
}

model ModelB for DbA {
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
                "name": "DbA"
            },
            "query": {
                "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.modelBId = ModelB.id WHERE ModelA.id = ?",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one"
            },
            "result": ""
        }
    ]
]
```

The runtime will execute the query with the provided `id` parameter (as instructed by `"args": {"from_params": ["id"]}`), and return the result of the `JOIN` as a single object in the `modelB` field. It will ask the ORM to map the result of the query to the `ModelB` model, and return it as a single object in the `modelB` field (as instructed by `"map": "one"`).

## One-to-Many Relationships

```cloesce
model ModelA for DbA {
    primary {
        id: int
    }

    many ModelB::modelAId(id) {
        modelBs
    }
}

model ModelB for DbA {
    primary {
        id: int
    }

    foreign ModelA::id {
        modelAId
    }
}
```

EX: `GET ModelA`

```json
[
    [
        {
            "db": {
                "name": "DbA"
            },
            "query": {
                "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.id = ModelB.modelAId WHERE ModelA.id = ?",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one"
            },
            "result": ""
        }
    ]
]
```

## Several Relationships

```cloesce
model ModelA for DbA {
    primary {
        id: int
    }

    foreign ModelB::id {
        modelBId
    }

    one ModelB::id(modelBId) {
        modelB
    }

    many ModelC::modelAId(id) {
        modelCs
    }
}

// ...
```

EX: `GET ModelA`

```json
[
    [
        {
            "db": {
                "name": "DbA"
            },
            "query": {
                "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.modelBId = ModelB.id JOIN ModelC ON ModelA.id = ModelC.modelAId WHERE ModelA.id = ?",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one"
            },
            "result": ""
        }
    ]
]
```

# Different Databases

## One-to-One Relationships

```cloesce
model ModelA for DbA {
    primary {
        id: int
    }

    column {
        modelBId: int
    }

    one ModelB::id(modelBId) {
        modelB
    }
}

model ModelB for DbB {
    primary {
        id: int
    }
}
```

```json
[
    [
        {
            "db": {
                "name": "DbA"
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one"
            },
            "result": ""
        }
    ],
    [
        {
            "db": {
                "name": "DbB"
            },
            "query": {
                "sql": "SELECT * FROM ModelB WHERE ModelB.id = ?",
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

This plan is executed by first querying `ModelA` in `DbA` with the provided `id` parameter. 

The result of that query is then used to query `ModelB` in `DbB` using the `modelBId` field from the result of the first query. The result of the second query is then mapped to the `modelB` field in the final result.

## One-to-Many Relationships

```cloesce
model ModelA for DbA {
    primary {
        id: int
    }

    many ModelB::modelAId(id) {
        modelBs
    }
}

model ModelB for DbB {
    primary {
        id: int
    }

    column {
        modelAId: int
    }
}
```

```json
[
    [
        {
            "db": {
                "name": "DbA"
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one"
            },
            "result": ""
        }
    ],
    [
        {
            "db": {
                "name": "DbB"
            },
            "query": {
                "sql": "SELECT * FROM ModelB WHERE ModelB.modelAId = ?",
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

## Several Relationships

```cloesce
model ModelA for DbA {
    primary {
        id: int
    }

    column {
        bId: int
    }

    one ModelB::id(bId) {
        modelB
    }

    many ModelC::modelAId(id) {
        modelCs
    }
}

// ...
```

```json
[
    [
        {
            "db": {
                "name": "DbA"
            },
            "query": {
                "sql": "SELECT * FROM ModelA WHERE ModelA.id = ?",
                "args": {
                    "from_params": ["id"]
                },
                "map": "one"
            },
            "result": ""
        }
    ],
    [
        {
            "db": {
                "name": "DbB"
            },
            "query": {
                "sql": "SELECT * FROM ModelB WHERE ModelB.id = ?",
                "args": {
                    "from_result": ["bId"]
                },
                "map": "one"
            },
            "result": "modelB"
        },
        {
            "db": {
                "name": "DbC"
            },
            "query": {
                "sql": "SELECT * FROM ModelC WHERE ModelC.modelAId = ?",
                "args": {
                    "from_result": ["id"]
                },
                "map": "many"
            },
            "result": "modelCs"
        }
    ]
]
```

In the second transaction, the runtime will execute two queries in parallel: one to fetch `ModelB` from `DbB`, and another to fetch `ModelC` from `DbC`. The results of these queries will then be mapped to the `modelB` and `modelCs` fields in the final result.

# Hybrid

`ModelA` and `ModelB` are in `DbAB` but `ModelC` is in `DbC`:

```cloesce
model ModelA for DbAB {
    primary {
        aPrimary: int
    }

    foreign ModelB::bPrimary {
        bForeign: int
    }

    one ModelB::bPrimary(bForeign) {
        modelB
    }

    column {
        cForeign: int
    }

    many ModelC::modelAId(aPrimary) {
        modelCs
    }
}

model ModelB for DbAB {
    primary {
        bPrimary: int
    }
}

model ModelC for DbC {
    primary {
        cPrimary: int
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
                "name": "DbAB"
            },
            "query": {
                "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.bForeign = ModelB.bPrimary WHERE ModelA.aPrimary = ?",
                "args": {
                    "from_params": ["aPrimary"]
                },
                "map": "one"
            },
            "result": ""
        }
    ],
    [
        {
            "db": {
                "name": "DbC"
            },
            "query": {
                "sql": "SELECT * FROM ModelC WHERE ModelC.modelAId = ?",
                "args": {
                    "from_result": ["aPrimary"]
                },
                "map": "many"
            },
            "result": "modelCs"
        }
    ]
]
```
