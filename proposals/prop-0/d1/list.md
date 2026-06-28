# Same Database

## One-to-One Relationship

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

It is clear that to `SELECT ModelA`, it must join `ModelB` on `ModelA.modelBId = ModelB.id`. After such, the result must be mapped from SQL rows to JSON. The Query Planner will generate the following query plan for a `list` operation on `ModelA`:

```json
[
  [
    {
      "db": {
        "name": "DbA"
      },
      "query": {
        "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.modelBId = ModelB.id",
        "map": "many"
      },
      "result": ""
    }
  ]
]
```

The runtime will execute this query as so:

1. Query the WASM ORM: `LIST ModelA`
2. Receive the query plan from the Query Planner
3. Execute `plan[0][0].query` on `plan[0][0].db` (in this case, `DbA`)
4. Gather results, ask the WASM ORM to map the results to JSON
5. Return all results as they are because `plan[0][0].map` is `many` and `plan[0][0].result` is empty (root)

Note that the `map` is `many` because this is a `list` operation.

## One-to-Many Relationship

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

In this case, `ModelA` has many `ModelB`s, and we must join `ModelB` on `ModelA.id = ModelB.modelAId`. The Query Planner will generate the following query plan for a `list` operation on `ModelA`:

```json
[
  [
    {
      "db": {
        "name": "DbA"
      },
      "query": {
        "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.id = ModelB.modelAId",
        "map": "many"
      },
      "result": ""
    }
  ]
]
```

The runtime will execute this query as so:

1. Query the WASM ORM: `LIST ModelA`
2. Receive the query plan from the Query Planner
3. Execute `plan[0][0].query` on `plan[0][0].db` (in this case, `DbA`)
4. Gather results, ask the WASM ORM to map the results to JSON
5. Return all results as they are because `plan[0][0].map` is `many` and `plan[0][0].result` is empty (root)

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

The query planner will generate a single query to fetch all `ModelA`s, `ModelB`s, and `ModelC`s in one go. The query plan will look like this:

```json
[
  [
    {
      "db": {
        "name": "DbA"
      },
      "query": {
        "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.modelBId = ModelB.id JOIN ModelC ON ModelA.id = ModelC.modelAId",
        "map": "many"
      },
      "result": ""
    }
  ]
]
```

# Different Databases

## One-to-One Relationship

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

Note that `ModelA` does not have a foreign key to `ModelB` because they are in different databases. The Query Planner will generate the following query plan for a `list` operation on `ModelA`:

```json
[
  [
    {
      "db": {
        "name": "DbA"
      },
      "query": {
        "sql": "SELECT * FROM ModelA",
        "map": "many"
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
        "sql": "SELECT * FROM ModelB WHERE id IN (?)",
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

After querying `LIST ModelA`, the runtime will receive the above plan, composed of two transactions. The first transaction will:

1. Execute `plan[0][0].query` on `plan[0][0].db` (in this case, `DbA`)
2. Gather results, ask the WASM ORM to map the results to JSON
3. Return all results as they are because `plan[0][0].map` is `many` and `plan[0][0].result` is empty (root)

EX: Result after T0 =>

```json
[
  {
    "id": 1,
    "modelBId": 1
  },
  {
    "id": 2,
    "modelBId": 2
  }
]
```

Another transaction exists and will be executed after the first transaction completes. The second transaction will:

1. For each value in the current result, gather chunks of 999 (the maximum number of parameters allowed in a D1 query), grouped by `modelBId` (remove duplicates)
2. For each batch, prepare the query `plan[1][0].query` on `plan[1][0].db` (in this case, `DbB`)
3. Execute all batches in one transaction (using D1 batch statement)
4. Gather results, ask the WASM ORM to map the results to JSON
5. For each result, find the corresponding `ModelA` in the first transaction's result and attach the `ModelB` to it. Attach only the first result because `plan[1][0].map` is `one`.

EX: Result after T1 =>

```json
[
  {
    "id": 1,
    "modelBId": 1,
    "modelB": {
      "id": 1
    }
  },
  {
    "id": 2,
    "modelBId": 2,
    "modelB": {
      "id": 2
    }
  }
]
```

No more transactions exist, and the runtime will return the final result to the caller.

- In order to make this work, the runtime _must_ dedupe the cross-database query parameters, because we use `IN (?)` which is a set, making us lose the ability to map the results back to the original query positions.

## One-to-Many Relationship

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

The Query Planner will generate the following query plan for a `list` operation on `ModelA`:

```json
[
  [
    {
      "db": {
        "name": "DbA"
      },
      "query": {
        "sql": "SELECT * FROM ModelA",
        "map": "many"
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
        "sql": "SELECT * FROM ModelB WHERE modelAId IN (?)",
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

The runtime will execute the first transaction as described in the previous section. After the first transaction completes, the second transaction will:

1. For each value in the current result, gather chunks of 999 (the maximum number of parameters allowed in a D1 query), grouped by `id` (remove duplicates)
2. For each batch, prepare the query `plan[1][0].query` on `plan[1][0].db` (in this case, `DbB`)
3. Execute all batches in one transaction (using D1 batch statement)
4. Gather results, ask the WASM ORM to map the results to JSON
5. For each result, find the corresponding `ModelA` in the first transaction's result and attach the `ModelB`s to it. Attach all results because `plan[1][0].map` is `many`.

EX T1 result =>

```json
[
  {
    "id": 1,
    "modelBs": [
      {
        "id": 1,
        "modelAId": 1
      },
      {
        "id": 2,
        "modelAId": 1
      }
    ]
  },
  {
    "id": 2,
    "modelBs": [
      {
        "id": 3,
        "modelAId": 2
      }
    ]
  }
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
        "sql": "SELECT * FROM ModelA",
        "map": "many"
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
        "sql": "SELECT * FROM ModelB WHERE id IN (?)",
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
        "sql": "SELECT * FROM ModelC WHERE modelAId IN (?)",
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

EX T1 result =>

```json
[
  {
    "id": 1,
    "bId": 1,
    "modelB": {
      "id": 1
    },
    "modelCs": [
      {
        "id": 1,
        "modelAId": 1
      },
      {
        "id": 2,
        "modelAId": 1
      }
    ]
  },
  {
    "id": 2,
    "bId": 2,
    "modelB": {
      "id": 2
    },
    "modelCs": [
      {
        "id": 3,
        "modelAId": 2
      }
    ]
  }
]
```

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

```json
[
  [
    {
      "db": {
        "name": "DbAB"
      },
      "query": {
        "sql": "SELECT * FROM ModelA JOIN ModelB ON ModelA.bForeign = ModelB.bPrimary",
        "map": "many"
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
        "sql": "SELECT * FROM ModelC WHERE modelAId IN (?)",
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

In this case, the first transaction will select the entire `ModelA` and `ModelB` tables, joining them on the foreign key. The second transaction will select all `ModelC`s that have a `modelAId` that exists in the first transaction's result.
