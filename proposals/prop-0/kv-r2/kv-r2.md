Any Model may have KV and R2 fields of any type. Fields may reference properties stored in SQLite, meaning their values can only be fetched after the SQLite query has been executed.

EX: `GET ModelA`:

```cloesce
kv Namespace {
    settings(id: string) -> string {
        "value/{id}"
    }
}

r2 Bucket {
    image(id: string) {
        "image/{id}"
    }
}

d1 {
    DbA
}

model ModelA for DbA {
    route {
        routeParam: string
    }

    primary {
        pk: string
    }

    r2 Bucket::image(routeParam) {
        imageFromRoute
    }

    kv Namespace::settings(routeParam) {
        settingsFromRoute
    }

    r2 Bucket::image(pk) {
        imageFromPk
    }

    kv Namespace::settings(pk) {
        settingsFromPk
    }
}
```

The above Model declares R2 and KV fields, some of which come from the `routeParam` and some of which come from the `pk`.

Because the Model may not exist, the ORM will first execute the SQLite query to fetch the table `ModelA` with the given `pk`. If the Model exists, the runtime will then execute the R2 and KV queries to fetch the values for the fields.

```json
[
  [
    {
      "db": {
        "name": "DbA"
      },
      "query": {
        "sql": "SELECT * FROM ModelA WHERE ModelA.pk = ?1",
        "args": {
          "from_params": ["pk"]
        },
        "map": "one"
      },
      "result": ""
    }
  ],
  [
    {
      "db": {
        "name": "Bucket"
      },
      "query": {
        "key": "image/{routeParam}",
        "args": {
          "from_result": ["routeParam"]
        }
      },
      "result": "imageFromRoute"
    },
    {
      "db": {
        "name": "Namespace"
      },
      "query": {
        "key": "value/{routeParam}",
        "args": {
          "from_result": ["routeParam"]
        }
      },
      "result": "settingsFromRoute"
    },
    {
      "db": {
        "name": "Bucket"
      },
      "query": {
        "key": "image/{pk}",
        "args": {
          "from_result": ["pk"]
        }
      },
      "result": "imageFromPk"
    },
    {
      "db": {
        "name": "Namespace"
      },
      "query": {
        "key": "value/{pk}",
        "args": {
          "from_result": ["pk"]
        }
      },
      "result": "settingsFromPk"
    }
  ]
]
```

In this example, every R2 and KV query can be executed in parallel, after SQLite is resolved.

# Durable Object KV

DO's have their own KV namespace, which could be sharded by some keys. Any Model may access the DO's KV namespace by providing the relevant shard keys (if any):

```cloesce
durable ShardedDo {
    shard {
        id: int
    }

    value(id: string) -> string {
        "value/{id}"
    }
}

model ModelA  {
    route {
        shard: int
        key: string
    }

    kv ShardedDo::{value(key), id(shard)} {
        valueFromRoute
    }
}
```

EX: `GET ModelA`:

```json
[
  [
    {
      "db": {
        "name": "ShardedDo",
        "args": {
          "from_params": ["shard"]
        }
      },
      "query": {
        "key": "value/{key}",
        "args": {
          "from_params": ["key"]
        }
      },
      "result": "valueFromRoute"
    }
  ]
]
```
