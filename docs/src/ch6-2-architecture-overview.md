# Architecture Overview

We can break down the Cloesce architecture into three components, each with their own subdomains: the Frontend, the Generator and the Migrations Engine.

## Frontend

The Frontend layer of Cloesce encompasses all components responsible for interfacing with the user’s project and extracting high level language into the Cloesce Interface Definition Language (CIDL). It serves as the both the entrypoint for compilation and the runtime environment for generated code.

### IDL Extraction

A key design choice when building Cloesce was to not force users to write their own models in a seperate IDL or DSL. Instead, we opted to have the Cloesce compiler utilize the source languages AST to extract model definitions directly from the user's source code. This allows users to define their models using familiar syntax and semantics, while still benefiting from the powerful features of Cloesce. Of course, this means we must write an extractor for each supported frontend language. Currently, the only supported language is TypeScript.

The Extractor portion of the compiler is responsible for scanning the user's source files (marked with `.cloesce.<lang>`) and identifying model definitions through stub preprocessor directives. Extraction does not perform any semantic analysis, it simply extracts the model definitions and their properties into an intermediate representation (CIDL).

The CIDL represents a full stack project. Every model definition, service, api endpoint and Wrangler binding is stored in the CIDL. Currently, this representation is serialized as JSON, but in the future we may explore other formats such as Protocol Buffers or FlatBuffers for better performance and extensibility.

At the end of CIDL extraction, a `cidl.pre.json` file is produced to semantically validated by the Generator.

### Runtime

Beyond extraction, the Frontend layer also includes the runtime environment for workers. Originally, we considered generating entirely standalone code for the workers, but a shift to instead interpret the CIDL as if it was the program text allowed us to greatly reduce the amount of generated code, add tests and improve maintainability. Each supported frontend language has its own runtime implementation that can interpret the CIDL using simple state machine logic. Moving as much of the runtime logic into WebAssembly as possible helps portability to other languages in the future.

The runtime consists of two components: the Router and the ORM. The Router is currently written entirely in TypeScript, while the ORM compiles to WebAssembly using Rust

#### Router

The Cloesce Router is responsible for handling incoming HTTP requests, matching them to an API endpoint defined in the CIDL, validating request parameters and body, hydrating data from the ORM and dispatching to a user defined method on a Model or Service. Along the way, the Router calls middleware functions defined in the CIDL. Although each middleware function can produce undefined behavior, each state in the Router is well defined and can produce only a corresponding failure state or success state. This makes reasoning about the Router's behavior straightforward.

#### ORM

The Cloesce ORM is responsible for fetching and updating data stored in SQL, KV and R2 according to the Model definitions and Include Trees passed. The majority of the ORM is written in Rust, however some portions are written in TypeScript such as KV and R2 hydration logic.

## Generator

After being passed the `pre.cidl.json` file from the Frontend, the Generator performs semantic analysis on the CIDL and Wrangler configuration to ensure that the project is valid. This includes checking for translatable SQLite types, sorting Models and Services topologically, validating API endpoints and more. If any errors are found, the Generator will output them to the user and halt compilation.

After semantic analysis is complete, the Generator produces the final `cidl.json` file which is then used to generate code for the worker runtime and client code generation. The generator will augment the CIDL with additional information like hashes for migrations and CRUD API endpoints for Models and Services.

## Migrations Engine

After a sucessful compilation, the Migrations Engine can be used to generate database migrations based on changes in the Model definitions, utilizing the hashes the Generator added to the CIDL. The Migrations Engine is responsible for creating valid SQL migrations that Wrangler can then apply to the D1 database.