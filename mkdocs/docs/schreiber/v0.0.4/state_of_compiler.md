# v0.0.4 State of the Compiler

Dumping system design as of v0.0.4 here. Cloesce is broken up into two main parts, each with their own sub-domains.

## Frontend

### Frontend Layer Overview

The Frontend layer of Cloesce encompasses all components responsible for interfacing with the user’s project, parsing their code, and preparing metadata for generation. It serves as the high-level entrypoint to the compiler, orchestrating the build process and executing the runtime logic necessary for code extraction and execution.

### Driver

The `Driver` contains everything a user needs to write and run a Cloesce project, exposing a command line interface for it's build system.

The driver is downloadable via a specific HLL's package manager, (currently, TypeScript + NPM, later Python + pip & Rust + cargo). The CLI is capable of orchestrating the entire compilation process through the exposed command:

```
cloesce compile
```

![compilation process](../../assets/Cloesce%20Compilation%20Stages.drawio.png)

In order to orchestrate the compilation process, the Driver must contain all necessary HLL code and WASM binaries. This means the Driver composes the UI backend + client, the ORM binary, and the Generator binary.

After the driver is installed, types and definitions can be retrieved (TS + NPM for ex):

```ts
import {
  D1,
  GET,
  POST,
  Inject,
  PrimaryKey,
  OneToMany,
  OneToOne,
  ForeignKey,
  IncludeTree,
  DataSource,
  modelsFromSql,
  WranglerEnv,
} from "cloesce/backend";
```

### UI (backend + client)

A library of types and definitions must be available to the developer such that they can both hint to the extractor their model design, and call upon Cloesce primitives, like ORM functions. Cloesce is split into two UI portions: backend and client.

#### Backend

The backend file exports all compiler hints (generally decorators), types (such as IncludeTree) and most contains a simple abstraction for invoking the WASM ABI through an ORM class.

The exported ORM class allows the invocation of:

- `object_relational_mapping`
- `upsert`

Through it's interface:

- `fromSql`
- `upsertQuery`

The ORM class also provides wrappers to make querying D1 as simple as possible:

- `upsert`
- `getQuery`
- `get`
- `listQuery`
- `list`

We aim to keep the HLL definition of the ORM minimal, and keep as much implementation in the universal WASM binary.

#### Client

The client file exports types needed on the frontend for the generated `client.ts` file to function, as well as for developers to have sufficient types. Mainly this is:

- types `HttpResult, Either, DeepPartial`
- generated code dependency `instantiateObjectArray`

### Extractor

The extractor is the stage in the compilation process in HLL project files are parsed, with all relevant compiler hints extracted into a form referred to as both the Cloesce Interface Definition Language (CIDL) and the Cloesce Metadata depending on the context (generator -> CIDL, runtime -> metadata).

The extractor searches for decorators (defined in the backend UI exports) in specific places, performing syntax analysis as it processes information.

Each HLL will require it's own unique implementation of the extractor, likely taking advantage of the languages pre-defined abstract syntax tree implementation to parse code.

### Router

As of now, each language requires it's own implementation of the `Cloesce Router Interface`. The goal of the CRI is to process incoming HTTP requests, undergoing a series of runtime type validation against the Cloesce Metadata, ultimately ending with a developer defined method implementation being called.

Methods in Cloesce can be:

- static
- instantiated
- CRUD

Static methods are those which simply operate in the namespace of the class. It's expected requests will come in the form `domain/api/Model/method` if the method is static. Static methods will require no hydration, meaning they need no database query to populate an object beforehand.

Instantaited methods are methods called on an instance of a class, expected to come in the form `domain/api/Model/:id/method`. Before the method is called, a database query is done along with a call to the `object_relational_mapping` function to create an actual instance of the class before the method is invoked.

Finally, CRUD methods are entirely generated methods with default implementations defined in the router. They include: `POST, PATCH, GET, LIST` (and maybe in the future `DEL`). CRUD methods will be intercepted by the router, unless some existing definition overrides it. They are expected to come in the form `domain/api/Model/METHOD` (with the exception of PATCH being an instantiated method).

![Cloesce Router](../../assets//Cloesce%20Router.drawio.png)

Since the CRI must be implemented in many languages, it will be important to follow the exact finite states as defined in the image above (the Cloesce Router Interface).

### ORM

Data hydration and CRUD generation are key features of Cloesce. Without data hydration, instantiated methods could not be invoked (making Cloesce nothing more than a fancy router), and without CRUD generation the philosophy of "write no more boilerplate/glue" isn't satisfied.

In an effort to avoid writing the complicated algorithms needed to accomplish these tasks in each HLL (leading to a nightmare of implementation inconsistencies), we opted to write a single implemenetation in Rust, and compile it to WASM.

The ORM is (currently) composed of two functions: `object_relational_mapping` and `upsert`.

`object_relational_mapping` takes an input of SQL columns, and transforms it into a JSON form that represents a model definition. In order to do this, it relies on the metadata to be passed from the Cloesce router into WASM memory. From this function, the Cloesce router is capable of traversing the JSON depth first and instantiating each object. If D1 ever supports more sophisiticated JSON procedures, we may some day be able to eliminate this code all together, in favor of our SQL view's natively producing JSON.

`upsert` takes a JSON representation of a Model and transforms that into insert, update and upsert SQL statements. It also relies on metadata to be passed in from the Cloesce router. Because of reasons discussed [here](./context_aware_insertions.md), all Cloesce SQL schemas must include a `cloesce_tmp` table to make the output execute entirely from SQL. A full exploration into the `upsert` function can be found in the previous link as well.

Because these functions are essential to the function of Cloesce, we categorize them as Cloesce primitives. Developers may use these indirectly functions via the backend UI.

## Generator

The Generator layer of Cloesce (or backend layer) is responsible for semantic analysis of the CIDL, along with producing final the compiled forms (being: SQLite, Wrangler.toml/jsonc, and HLL files) that form a Cloesce program.

### CLI

The Generator's main point of interaction is through it's CLI, exposing commands to generate each domain of the Generator, as well as commands to generate all domains sequentially (the compilation process). The CLI is also responsible for outputting errors in a readable fashion.

### Shallow Semantic Analysis

The CIDL enables an expressive grammar of nullability, arrays, void types, models, partials, etc, which could produce an illogical expression. As shown in the diagram in the Driver section, the generator is responsible for conducting semantic analysis of the CIDL.Before any specific domain of the Generator is compiled, analysis will catch:

1. Model attributes with invalid SQL types (can't map to SQL column)
2. Primary keys with invalid SQL types
3. Ill formatted lookup tables (keys should map to a related value)
4. Unknown navigation property references (model does not exist)
5. Unknown model references
6. Invalid parameter types (such as a `void` typed parameter)

### D1 / Sqlite + Full Semantic Analysis

After validation, the first stage in the generator is Sqlite generation, as it performs an in-depth analysis of the foreign keys / navigation properties of models. Analysis will catch:

1. Unknown or invalid foreign key references
2. Missing navigation property attributes
3. Cyclical dependencies
4. Invalid data sources (types, references)

Unlike the validation stage, this validation work is not seperated, but instead done at the same time as the actual compilation to SQLite runs, so as to avoid extra work.

The primary goal of SQLite generation is to transform a CIDL Model into an actual SQL table, considering all of its foreign key dependencies (be it 1:1, 1:M or M:M). Along with this, Cloesce utilizes "DataSources" which are essentially SQL views which describe how tables should be joined in data hydration. Data sources are expressed as graphs (which may be cyclical) so as to avoid infinite inclusion of models (ex: Person has many Persons results in a chain of `Person: { persons: [ {persons: []}...]}`). The link [here](../v0.0.2/thoughts_fks_v0.0.2.md) dives into foreign keys more.

### Wrangler

Cloesce will augment or create a Wrangler file that matches the form described in the CIDL. We don't want to replace the existing Wrangler config, but only supply the required values to run the project _if_ they are missing (databases, main entrypoint, compatability date, project name).

### Client API

One of the most powerful parts of Cloesce is it's ability to create a transparent API to call Workers endpoints. For example, the method `Person.speak()` defined on the backend would exist with the exact same parameters and return types on the frontend, albeit with an added async def and `HttpResult` wrapper. The Generator takes in the CIDL and produces an API to call the backend exactly as it has been defined, along with CRUD methods.

### WASI Artifact

Currently, in an effort to not compile the Generator to several different operating systems and create a CI/CD pipeline to manage them, Cloesce compiles the Generator to WASI, creating a universal binary that can be easily shipped and downloaded by the Driver. We may or may not stick with this in the future, but it's certainly the easiest way to go about publishing the binary.
