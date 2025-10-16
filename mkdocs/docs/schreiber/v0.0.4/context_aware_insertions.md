# Context Aware ORM Insertions + Partial Objects

## Objective

Inserting new a model into the database should be easy. In fact, it should be as easy as just (pseudocode):

```ts
let model = ...;
orm.insert(model, data_source);
```

Given a model that is completely filled out (all values are present), this isn't too bad of an algorithm to make. However, when a developer wants to insert a model into the database, they _usually don't_ know the primary key of that model (why would they when PK's can be auto generated?).

A potential solution to this is finding the last id inserted:

```ts
let max_id = db.query("SELECT MAX(id) from ...");
let model =  { id: max_id + 1, ...};
```

I'm not a fan of this as we've added an extra DB query and more code just to insert our value.

Further, when models have complicated relationships (such as Person has many Dogs), we would have to get new id's for every single model. As the complexity of the model grows, as does the complexity for the insert query, having to prefetch and calculate ID's beforehan. We'd end up with something like:

```ts
let max_person_id = ...
let max_dog_id = ...
let max_toys_id = ...

let model = {
    id: max_person_id + 1,
    dogs: [
        {
            id: max_dog_id + 1
            personId: max_person_id + 1,
            toys: [
                {
                    dog_id: max_dog_id + 1,
                    id: max_toys_id + 1
                },
                .
                .
                .
            ]
        }
        .
        .
        .
    ]
}
Orm.insert(model, data_source);
```

This sucks. Insert should really be as simple as _insert the model_.

## Giving inserts context

Ideally, primary keys and foreign keys are completely optional. We should just be able to either:

1. infer foreign keys from the surrounding context
2. assert that a missing primary key is a generated primary key, and keep that in our context.

This algorithm can get tricky. Note that insertions must occur in topological order, meaning that dependencies are inserted before dependents. For my implementation, I won't topo sort models beforehand, but instead traverse the models in a depth first tree-like manner such that the insertions (an accumulated list) are in topo order.

For this to work, we need a way to propogate information from a parent node to a child note, as well as from a child node to a parent node.

My approach was to use dynamic scoping (just as some compilers do). This is just a map with the keys being the exact path we've traversed to get to some value, and the value being either generated (which we will discuss later), or provided in the original model that is being inserted.

We will use this dynamic scope context to fill any missing values in the model. Of course, models can be ill-formatted and we might be unable to infer a value, in which case we just yield an error.

### 1:1

In the case of `Person` has a `Dog`, where `Person` does not know the `Dog's` id, we are traversing in a depth first order `Person -> Dog`, and inserting in a topological order `Dog -> Person`. Traversal wise, `Dog's ID` must **propogate upwards** to `Person`.

Again, for our dynamic scope, each key is the path that we have traversed, which uses the metadata to name values as if they were objects.

```
Init:
Scope = {}

Traverse: Person (Root)
Scope (add) => { Person.id: <some value> }

Traverse: Dog (Person.dog)
Scope (add)=> { Person.dog.id: 1, Person.id: <some value> }

Traverse: Person (Root)
Scope = { Person.dog.id: 1, Person.id: <some value>}


=> Model {
    id: <supplied>
    dog_id: 1
}
```

### 1:M

In the case of `Person` has many `Dogs`, where `Dog` does not know the `Person's` id, we must traverse in the order `Person`, `Dog` (as `Dog` is dependent on `Person` existing). Again, we are traversing depth first, so the actual order we traverse will be `Person, Dog`. This means all `Person` has to do is propogate it's id fowards to `Dog`.

```
Init:
Scope = {}

Traverse: Person (Root)
Scope (add)=> { Person.id: 1}

Traverse: Dog (Person.dogs)
Scope (add)=> { Person.id: 1, Person.dogs.id }

Traverse: Person (Root)
Scope (add)=> { Person.id: 1}


=> Model {
    id: <supplied>
    dogs: [
        {
            id: <supplied>
            person_id: 1
        }
    ]
}
```

Note: If person has many dogs, each time a dog is traversed, it will overwrite path `Person.dogs.id`'s value' in the scope. This is fine, because the id will only be relevant for that specific dogs tree traversal

### M:M

Many to Many is the same exact idea as `1:M`, however we need to add a junction table as well as capture the propogated values. The junction table is of `Parent, Child`, so we have everything we need to know.

## Adding generated values to context

It's not enough to just fetch foreign keys from context, it's also important that generated primary keys can be fetched as well. Unfortunately, Cloudflare makes this really tricky...

How do we figure out what the generated ID is? SQLite exposes a way to access the last inserted row id, but that is general across all tables, so for something like `1:M` the value would keep getting overwritten. You could also try to get the max row id as described before, but then you run into problems where cyclical relationships break: if `Person` has many `Persons`, each time they select the max row id it would be the id of the previously inserted `Person`, not necessarily the parent.

It's clear we have to store the result of an inserted rows id such that it is constant and can be used in subsequent expressions. SQLite provides Common Table Expressions to do exactly that. I even [implemented CTE's](https://github.com/bens-schreiber/cloesce/blob/schreiber/orm-ctes/src/runtime/src/methods/insert.rs)! Unfortunately, **Cloudflare D1 does not support CTE's that write to the database**. I don't know why this is the case, and I can't find any documentation that explicitly states that is the case, but after doing my own tests on my local machine and the D1 console, it's clear they aren't supported (once I start working there again I'm going to have to figure out why that is the case).

An alternative to this would be breaking our one magnificient insert transaction into many seperate queries to the database, essentially making the amount of internet requests to D1 size `N`. That sucks.

I explored `temporary tables`, which SQLite supports, but of course, **Cloudflare does not let you modify the schema outside of migrations**. What ever will we do.

## Adding a variables table to the Cloesce Schema

The only option I can see is shipping Cloesce with a `_cloesce_tmp` table, apart of every schema from here on out. It is comprised of a primary key (the path to some value as shown in the examples above), and a value (the id after insertion). With this, we can now add auto generated keys to our context, augmenting our queries to select from the `_cloesce_tmp` table when the context is auto generated instead of provided.

An important note with the temp table is that all values are removed after the transaction is complete. No remnants of the table will ever make it out of the query.

One last issue was encountered with this solution. After an insert query, it's useful to return the primary key of the inserted value. However, if the last statement that has to be made deletes all values from the temporary table, and the primary key is _in_ the temporary table, we have to make a creative query, that looks like:

```sql
WITH Person_id as (
    SELECT id from _cloesce_tmp WHERE path = 'Person.id'
)
DELETE FROM _cloesce_tmp RETURNING (SELECT id FROM Person_id)
```

Great, at least it works right? No. **Cloudflare does not let you put expressions inside of a RETURNING statement**. Again, I can't find any documentation on this, but testing on my local machine and on the D1 console returns a literal string `(SELECT id FROM "Person.id")`.

The solution for this one is on the backend runtime. Before deleting the table, the ORM function will provide a line:

```sql
SELECT id from _cloesce_tmp WHERE path = 'Person.id';
--- or, if the id was provided as a value
SELECT id as <some v>;
```

It's then up to the runtime code to turn our entire query into a batch query, search for the line that starts with `SELECT`, and make sure that result is returned. The backend ORM code is simple at least:

```ts
  async insert<T extends object>(
    ctor: new () => T,
    newModel: T,
    includeTree: IncludeTree<T> | null,
  ): Promise<Either<string, any>> {
    let insertQueryRes = Orm.insertQuery(ctor, newModel, includeTree);
    if (!insertQueryRes.ok) {
      return insertQueryRes;
    }

    // Split the query into individual statements.
    const statements = insertQueryRes.value
      .split(";")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);

    // One of these statements is a "SELECT", which is the root model id stmt.
    let selectIndex: number;
    for (let i = statements.length - 1; i >= 0; i--) {
      if (/^SELECT/i.test(statements[i])) {
        selectIndex = i;
        break;
      }
    }

    // Execute all statements in a batch.
    const batchRes = await this.db.batch(
      statements.map((s) => this.db.prepare(s)),
    );

    if (!batchRes.every((r) => r.success)) {
      const failed = batchRes.find((r) => !r.success);
      return left(
        failed?.error ?? "D1 batch failed, but no error was returned.",
      );
    }

    // Return the result of the SELECT statement
    const selectResult = batchRes[selectIndex!].results[0] as { id: any };

    return right(selectResult.id);
  }
```

With that, all a developer has to do to insert a model is:

```ts
let model = ...;
orm.insert(model, data_source);
```

## Partial

Since the ORM can insert partial objects, we need some way for Cloesce to recognize that an object is partial (don't reject it from the request validator stage if it's missing some values). To this end, I added the `Partial` grammar to the CIDL, and then updated the extractor and TS runtime to use it accordingly.
