# Transition to Web Assembly

In `v0.0.2` we moved away from generating language specific code to run the workers backend by favoring TypeScript. In `v0.0.4`, we would like to move away from TypeScript, attempting to write the majority of our `cloesce` function in Rust.

## `cloesce.ts` -> `cloesce.rs`

The `cloesce` function is a simple state machine:

```
initial             (some incoming request)


    |
    V

Match Route         (Extract a model method, data source from request)


    |
    V

Validate Request    (Validate the request body / url params to the model method)


    |
    V

Hydrate Model       (Hydrate an instance of the model using the data source, or skip for static methods)


    |
    V

Method Dispatch     (Run the method on the model)
```

Importantly, in the `Hydrate Model` step, we will run a rather complex algorithm `modelsFromSql` which maps an ORM friendly view result (flat, no JSON) to a JSON object of the model.

What states can we turn into WASM? If we assume there is some way to serialize the incoming request to something WASM can understand, as well as the cidl, match route and validate request can be completely moved to web assembly.

For model hydration, WASM will limit us by not allowing database calls within it, so some TypeScript code must still exist as synchronous callbacks that we pass into WASM. `modelsFromSql` should be capable of being moved to WASM as well, again assuming we can serialize data well enough.

Finally, method dispatch will have to be entirely TypeScript, as we create an object and call a method from the information we've gathered, and use dependency injection for the information we lack.

## WASM ORM (worm)

An important part of this milestone is creating a simple ORM for users to do basic operations with, being Get, List, Post, Patch, Delete methods. `modelsFromSql` already implements Get and List, so we need to create some other functions for posting (inserting a model into a table), patching (updating a record in the table) and deleting (removing a record from a table).

It will be valuable to have these methods in WASM as it will be annoying maintain to both writing all of these methods in every language, and updating every one of them whenever some bug or featue is added.

To this end, our WASM binary needs to expose an interface such that any HLL can implement an intuitive UI to interact with it.

To arrive at this WASM interface, let's go backwards and imagine a TypeScript UI:

```ts

@D1
class Foo {
    @DataSource
    static readonly default {...}

    ...
}

// init repository
let models = ModelRepository.from(db);

// GET a model
{
    let foo: DbResult<Foo> = await models.get(Foo, Foo.default, 0); // id 0
}

// LIST models
{
    let foos: DbResult<Foo[]> = await models.list(Foo, Foo.default);
}

// GET with custom query
{
    let foo: DbResult<Foo> = await models.get(
        Foo,
        Foo.default,
        (db) =>
            db.prepare('SELECT * FROM [Foo.default] WHERE id = ? and ...')  // stmt that returns a Foo
            .bind(id)
    );
}

// POST a model
{
    let newFoo = {...};
    let foo = await models.post(Foo, Foo.default, newFoo);
}

// PATCH a model
{
    let updatedFoo: Partial<Foo> = {...};
    let foo = await models.patch(Foo, Foo.default, 0, updatedFoo);
}

// PATCH with custom query
{
    let updatedFoo: Partial<Foo> = {...};
    let foo = await models.patch(
        Foo,
        Foo.default,
        updatedFoo,
        (db) => db.prepare.('SELECT [Foo.id] FROM [Foo.default] WHERE ...') // stmt that returns Foo Id to be patched
    );
}

// DELETE a model
{
    let foo = await models.post(Foo, Foo.default, 0);
}

// DELETE a model with custom query
{
    await models.delete(
        Foo,
        (db) => db.prepare.('SELECT [Foo.id] FROM [Foo.default] WHERE ...') // stmt that returns a Foo Id to be deleted
    );
}
```

From this, it seems like the interface WASM must support (in pseudo) is:

```c#

// covers get
//
// If queryString is not empty, the use the query and then just try to map the result to a `modelName`
// If id is not empty then use a default query "SELECT * FROM [modelName.dataSourceName] WHERE modelName.pk = id"
//
// Returns a JSON string, or some error
virtual Buffer selectModel(Buffer modelName, Buffer dataSourceName, Buffer meta, Buffer queryString, Buffer id);

// covers list
//
// If queryString is not empty, the use the query and then just try to map the result to a `modelName`
// Else, use a default query "SELECT * FROM [modelName.dataSourceName]"
//
// Returns a JSON string, or some error
virtual Buffer selectModels(Buffer modelName, Buffer dataSourceName, Buffer meta, Buffer queryString, Buffer id);

// covers post
//
// newModel is the JSON of some modelName
//
// Returns a JSON string, or some error
virtual Buffer insertModel(Buffer modelName, Buffer dataSourceName, Buffer meta, Buffer newModel);

// covers patch
//
// partialModel is the parts of the model that should be updated
//
// Returns a JSON string, or some error
virtual Buffer updateModel(Buffer modelName, Buffer dataSourceName, Buffer meta, Buffer partialModel, Buffer queryString, Buffer id);

// covers delete
//
// If queryString is not empty, the use the query to get the id
// Else, use a default query "DELETE FROM [modelName] where [modelName].[pk] = id"
//
// Returns some error if occured
virtual Buffer deleteModel(Buffer modelName, Buffer dataSourceName, Buffer meta, Buffer queryString, Buffer id);
```

### Patch

With this patch setup, only a `Partial<T>` is required to update something. In the future, we could create a change tracker that automates this.

## Serializing WASM Parameters

From the previous section, the WASM interface will take the string values:

- `modelName`
- `dataSourceName`
- `queryString`

The JSON values:

- `meta`

and the variable value:

- `id` (could be string, float, int, even blob (i'm not too worried about supporting BLOB PK's))

WASM only supports linear memory arrays, so all values will have to be appropriately serialized and deserialized. This means not only does the Rust code we use to create the WASM binary have to take in memory arrays, but the HLL code needs to serialize to it. This could all be a pain, but a neat project [wit-bindgen](https://github.com/bytecodealliance/wit-bindgen) automates this entirely-- we just create an interface like we have done above in their IDL and then can generate all of the files necessary. We wouldn't have to generate these files on compilation too, as the interface is static and will not change!

Serializing and deserializing the entire Cloesce AST is relatively costly, especially when chaining multiple calls together. We can cheat by using data sources: the only part of the AST that is necessary are models that are in the data sources inclue tree, and we only even need the attributes and FK's. That will reduce the process from `O(AST_Size)` to `O(IncludeTreeDepth)`, which is a significant difference.

## Flow of the `cloesce` backend

We will aim to implement as much logic as possible in WASM. It turns out just about everything can be put into WASM, excluding the database call and the model dispatch.

So, the states will be exactly as they are now. On the TypeScript or other HLL side, we will just create a function pointer:

```ts
function d1Callback(query, id) {
  let records;
  try {
    records = await d1.prepare(query).bind(id).run();
    if (!records) {
      return missingRecord;
    }
    if (records.error) {
      return malformedQuery(records.error);
    }
  } catch (e) {
    return malformedQuery(e);
  }
}
```

although this may need to be modified to throw errors so the WASM binary can exit correctly.

From a high level, the flow is more or less:

```ts
function runBackend(request, ...) {
    const res = wasm.cloesce(request, ast, d1Callback);
    if (!res.ok) {
        return error(res.message);
    }

    const { model, method,  value} = res.data;

    const instantiate = instantiateModel(value);
    methodDispatch(instantiate, ...)
}

```
