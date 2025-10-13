# cloesce (experimental, `v0.0.3`)

Cloesce is a full stack compiler for the Cloudflare developer platform, allowing class definitions in high level languages to serve as the metadata basis for databases, a backend REST API, a frontend API client, and Cloudflare infrastructure.

Cloesce is working towards an alpha MVP (v0.1.0), with the general milestones being [here](https://cloesce.pages.dev/schreiber/v0.1.0_milestones/).

Internal documentation going over design decisions and general thoughts for each milestone can be found [here](https://cloesce.pages.dev/).

# Documentation `v0.0.3`

Note that this version is very unstable (ie, it passes our set of happy-path tests).

## Getting Started

`v0.0.3` supports only Typescript-to-Typescript projects. An example project is shown [here](https://github.com/bens-schreiber/cloesce/tree/main/examples).

1. NPM

- Create an NPM project with the `cloesce` pkg:

```json
    "cloesce": "^0.0.3-fix.2",
```

2. TypeScript

- Create a `tsconfig.json` with the following values:

```json
{
  "compilerOptions": {
    // ...
    "resolveJsonModule": true,
    "strict": true,
    "strictPropertyInitialization": false,
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true
  },
  "include": ["<your_src_dir>/**/*.ts", ".generated/*.ts"]
}
```

3. Cloesce Config

- Create a `cloesce.config.json` with the following values:

```json
{
  "source": "./src", // or whatever src you want
  "workersUrl": "http://localhost:5002/api", // or whatever url you want
  "clientUrl": "http://localhost:5002/api"
}
```

4. Vite

- `v0.0.3` does not yet support middleware, so you'll run into CORs problems. A vite proxy in some `vite.config.ts` can fix this:

```ts
import { defineConfig } from "vite";

export default defineConfig({
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://localhost:5000",
        changeOrigin: true,
      },
    },
  },
});
```

5. Wrangler Config

- `v0.0.3` will generate the required areas of your wrangler config. A full config looks like this:

```toml
compatibility_date = "2025-10-02"
main = ".generated/workers.ts"
name = "example"

[[d1_databases]]
binding = "db"
database_id = "..."
database_name = "example"
migrations_dir = ".generated/migrations"
```

## A Simple Model

A model is a type which represents a database table, database view, backend api, frontend client and Cloudflare infrastructure (D1 + Workers).

Suprisingly, it's pretty compact. A basic scalar model (being, without foreign keys) looks like this:

```ts
// horse.cloesce.ts

@D1
export class Horse {
  @PrimaryKey
  id: number;

  name: string | null;

  @POST
  async neigh(): Promise<string> {
    return `i am ${this.name}, this is my horse noise`;
  }
}
```

- `@D1` denotes that this is a SQL Table
- `@PrimaryKey` sets the SQL primary key. All models require a primary key.
- `@POST` reveals the method as an API endpoint with the `POST` HTTP Verb.

After running `cloesce run`, you will get a fully generated project that can be ran with:

```sh
# migrate wrangler
npx wrangler d1 migrations apply proj-name

# build
npx wrangler build

# run wrangler
npx wrangler dev --port 5000
```

Note the output in the `.generated/` dir:

- `client.ts` is an importable API with all of your backend types and endpoints
- `migrations/*.sql` is the generated SQL code (note, migrations beyond the initial aren't really supported yet)
- `workers.ts` is the workers entrypoint.

## Features

### Wrangler Environment

In order to interact with your database, you will need to define a WranglerEnv

```ts
// horse.cloesce.ts

@WranglerEnv
export class Env {
  db: D1Database; // only one DB is supported for now-- make sure it matches the name in `wrangler.toml`

  // you can also define values in the toml under [[vars]]
  motd: string;
}
```

The wrangler environment is dependency injected into your method calls:

```ts
@D1
export class Horse {
  @PrimaryKey
  id: number;

  @POST
  async neigh(@Inject env: WranglerEnv): Promise<string> {
    env.db.prepare(...);

    return `i am ${this.name}, this is my horse noise`;
  }
}
```

## Foreign Keys, One to One, Data Sources

More complex model relationships are permitted via the `@ForeignKey`, `@OneToOne / @OneToMany @ManyToMany` and `@DataSource` decorators.
Foreign keys are scalar attributes which must reference some other model's primary key:

```ts
@D1
export class B {
  @PrimaryKey
  id: number;
}

@D1
export class A {
  @PrimaryKey
  id: number;

  @ForeignKey(B)
  bId: number;
}
```

This representation is true to the underlying SQL table: `A` has a column `bId` which is a foreign key to `B`. Cloesce allows you to actually join these tables together in your model representation:

```ts
@D1
export class B {
  @PrimaryKey
  id: number;
}

@D1
export class A {
  @PrimaryKey
  id: number;

  @ForeignKey(B)
  bId: number;

  @OneToOne("bId") // references A.bId
  b: B | undefined; // This value is a "navigation property", which may or may not exist at runtime
}
```

In `v0.0.3`, there are no defaults, only very explicit decisons. Because of that, navigation properties won't exist at runtime unless you tell them to. Cloesce does this via a `DataSource`, which describes the foreign key dependencies you wish to include. All scalar properties are included by default and cannot be excluded.

```ts
@D1
export class B {
  @PrimaryKey
  id: number;
}

@D1
export class A {
  @PrimaryKey
  id: number;

  @ForeignKey(B)
  bId: number;

  @OneToOne("bId")
  b: B | undefined;

  @DataSource
  static readonly default: IncludeTree<A> = {
    b: {}, // says: on model population, join A's B
  };
}
```

Datasources are just SQL views and can be invoked in your queries. They are aliased in such a way that its identical to object properties. The frontend chooses which datasource to use in it's API client. `null` is a valid option, meaning no joins will occur.

```ts
@D1
export class A {
  @PrimaryKey
  id: number;

  @ForeignKey(B)
  bId: number;

  @OneToOne("bId")
  b: B | undefined;

  @DataSource
  static readonly default: IncludeTree<A> = {
    b: {},
  };

  @GET
  static async get(id: number, @Inject env: WranglerEnv): Promise<A> {
    let records = await env.db
      .prepare("SELECT * FROM [A.default] WHERE [A.id] = ?") // A.default is the SQL view generated from the IncludeTree
      .bind(id)
      .run();

    // modelsFromSql is a provided function to turn sql rows into an object.
    // More ORM functions will be expanded on in v0.0.4.
    return modelsFromSql(A, records.results, A.default)[0] as A;
  }
}
```

Note: In later versions, nearly all of the code from the example above won't be needed (we can usually infer primary keys, foreign keys, relationships, have default made include trees, and generate CRUD methods like `get`).

### One to Many

Cloesce supports models with `1:M` relationships:

```ts
@D1
export class Person {
  @PrimaryKey
  id: number;

  @OneToMany("personId") // directly references the FK on Dog
  dogs: Dog[];

  @DataSource
  static readonly default: IncludeTree<Person> = {
    dogs: {
      person: {
        dogs: {
          // essentially means: "When you get a person, get their dogs, and get all of those dog's Person, ..."
          // we could go on as long as we want
        },
      },
    },
  };
}

@D1
export class Dog {
  @PrimaryKey
  id: number;

  @ForeignKey(Person)
  personId: number;

  // optional navigation property, not needed.
  @OneToOne("personId")
  person: Person | undefined;
}
```

### Many to Many

NOTE: `M:M` relationships have a [bug](https://github.com/bens-schreiber/cloesce/issues/88) in `v0.0.3`, but the syntax is as follows:

```ts
@D1
export class Student {
  @PrimaryKey
  id: number;

  @ManyToMany("StudentsCourses") // unique ID for the generated junction table
  courses: Course[];
}

@D1
export class Course {
  @PrimaryKey
  id: number;

  @ManyToMany("StudentsCourses")
  students: Student[];
}
```

### Plain Old Objects

Simple non-model objects can be returned and serialized from a model method:

```ts
@PlainOldObject
export class CatStuff {
    catFacts: string[],
    catNames: string[],
}

@D1
export class Cat {
    @PrimaryKey
    id: number;

    @GET
    async query(): Promise<CatStuff> {
        return {
            catFacts: ["cats r cool"],
            catNames: ["reginald"]
        }
    }
}
```

### HttpResult

Methods can return any kind of status code via the `HttpResult` wrapper:

```ts
@D1
class Foo {
    ...

    @GET
    async foo(): Promise<HttpResult<number>> {
        return { ok: false, status: 500, message: "divided by 0"};
    }
}
```

# Testing the Compiler

## Unit Tests

- `src/extractor/ts` run `npm test`
- `src/generator` run `cargo test`

## Integration Tests

- To run the regression tests: `cargo run --bin test regression`
- To run the pass fail extractor tests: `cargo run --bin test run-fail`

Optionally, pass `--check` if new snapshots should not be created.

## E2E

- `tests/e2e` run `npm test`

## Code Formatting

- `cargo fmt`, `cargo clippy`, `npm run format:fix`
