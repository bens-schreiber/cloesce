# State of the Compiler (v0.0.2)

As of 10/26/2025, we have just finished our second milestone. In v0.0.2, we introduced

- Foreign key relationships
- Data sources with include trees
- Method return types
- Navigation properties

In creating these features, we ran into an interesting problem with the data hydration stage discussed in [this architecture model](https://cloesce.pages.dev/kotschevar-smead/thoughts_api_v0%2C0%2C1/#architecture). To give context on the issue, it's important to understand how data sources work in Cloesce. Users define [Include Trees](https://cloesce.pages.dev/schreiber/thoughts_fks_v0.0.2/#problems-with-naive-view), which describe which navigation properties on a model should be populated from a SQL query (navigation properties represent joined foreign key relationships on a query). Cloesce turns these user defined trees into unambigious SQL views, which return simple flattened columns such that a more sophisiticated algorithm can turn these columns into a Model representation.

Before v0.0.2, models had no complex relationships, and could be easily "hydrated" into models, for example:

```
ToModel<Person>({id: 0, name: "julio", lastname: "pumpkin"}) => Person {id: 0, name: ...}
```

With more complex relationships comes ambiguity in the results. For example, say Person has two Dogs, and Dog also has a Dog. Assume our data source is set to include all relationships, after querying the generated `Person_default` view, we would get something like

```
[{Person_id: 0, Person_name: "julio", Person_lastname: "pumpkin", Dog1_id: 0, Dog2_id: 1, Dog3_id: 2, Dog4_id: 3}]
```

Without knowing exactly what the `Person` type's attributes and relationships are, it's not possible to create an instantiated `Person` from this result. You can try to return better names from the SQL view, like `Person_Dog1, Person_Dog2, Dog1_Dog, Dog2_Dog...` but that doesn't work when considering One to Many or Many to Many relationships where someone could have N dogs.

Some projects solve this issue by returning JSON structured results from the database, like:

```json
[
    {
        Person_id: ...
        Person_name: ...
        Person_lastname: ...
        Person_dogs: [
            {
                Dog_id: ...
            }
            ...
        ]
    }
]
```

however, we are limited by SQLite's JSON tools-- this isn't really something SQLite should do. Thus, we need to have some kind of intelligent algorithm on the backend that is capable of knowing the structure of a Model. TypeScript's types are eliminated at runtime, so we can't use any reflection based strategy. Other [TypeScript ORM's](https://typeorm.io) solve this by creating metadata with decorators:

```
@Entity()
export class User {
    @PrimaryGeneratedColumn()
    id: number

    @Column()
    firstName: string

    @Column()
    lastName: string

    @Column()
    age: number
}
```

Cloesce is already doing this, but instead of creating the metadata in TypeScript, we extract the metadata to a JSON file. It's clear that this metadata we extract for the compiler is also going to be necessary for the backend to do data hydration, and thus the solution is to link the generated CIDL json file at runtime, and then use that for type and relationship metadata.

This worked great, but it got us thinking: if we need the CIDL to exist at runtime, why are we even generating TypeScript code in Rust for the backend? What if we were to create some function, say `cloesce` that takes the CIDL as a parameter, some incoming request as a parameter, then does the same state machine we currently generate? There would be a _ton_ of benefits:

1. We can't easily test the rust code that we generate, only through complex end to end tests and gross snapshot tests. A determinsitic TypeScript function can be unit tested!
2. The Rust portion would only have to do file linking: everything else can be done with an abstract CIDL input.
3. The TypeScript portion would take in a compiler verified CIDL, and know it will not run into any missing/invalid property errors
4. Far easier development experience, and it's even easier to add new languages to Cloesce: just create a small backend.

So, we did exactly that. All of the old Rust code to generate a TypeScript state machine was scrapped to just call the `cloesce` function, and we now have a single file `cloesce.ts`, which utilzes all CIDL metadata to route, validate, hydrate and dispatch user defined code.

TypeScript (or really, JavaScript) is the perfect language for this kind of type dynamism. An important addition to our generated typescript code aside from file linking, is also creating a `Constructor Registry` which is capable of mapping a Model name to it's user defined definition, ie `{"Person": Person}`.

Looking at other languages we will support, Python can implement our TypeScript setup essentially one to one. Rust on the other hand will be more tricky, but certainly possible.

## v0.0.3

Currently we are in a good state to make test projects and play around with what we've built, so the next milestone will involve hunting down bugs from v0.0.2. Other than that, v0.0.3 is mostly an expansion on v0.0.2, adding more return types and reducing the amount of boilerplate needed.
