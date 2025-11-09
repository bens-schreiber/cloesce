# cloesce (unstable, `v0.0.4`)

Cloesce is a full stack compiler for the Cloudflare developer platform, allowing class definitions in high level languages to serve as a metadata basis to create a database schema, backend REST API, frontend API client, and Cloudflare infrastructure (as of v0.0.4, D1 + Workers).

Cloesce is working towards a stable alpha MVP (v0.1.0), with the general milestones being [here](https://cloesce.pages.dev/schreiber/v0.1.0_milestones/).

Internal documentation going over design decisions and general thoughts for each milestone can be found [here](https://cloesce.pages.dev/).

# Documentation `v0.0.4`

## Getting Started

`v0.0.4` supports only Typescript-to-Typescript projects. An example project is shown [here](https://github.com/bens-schreiber/cloesce/tree/main/examples).

1. NPM

- Create an NPM project and install cloesce

```sh
npm i cloesce@0.0.4-unstable.5
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

- Create a `cloesce.config.json` with the following keys:

```json
{
  "source": "./src",
  "workersUrl": "http://localhost:5000/api",
  "clientUrl": "http://localhost:5173/api"
}
```

4. Vite

To prevent CORS issues, a Vite proxy can be used for the frontend:

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

- `v0.0.4` will generate the required areas of your wrangler config. A full config looks like this:

```toml
compatibility_date = "2025-10-02"
main = ".generated/workers.ts"
name = "example"

[[d1_databases]]
binding = "db"
database_id = "..."
database_name = "example"
```

## A Simple Model

A model is a type which represents:

- a database table,
- database views
- REST API
- Client API
- Cloudflare infrastructure (D1 + Workers)

Suprisingly, it's pretty compact. A basic model looks like this:

```ts
// horse.cloesce.ts
import { D1, GET, POST, PrimaryKey } from "cloesce/backend";

@D1
export class Horse {
  @PrimaryKey
  id: number;

  name: string | null;

  @POST
  neigh(): string {
    return `i am ${this.name}, this is my horse noise`;
  }
}
```

- `@D1` denotes that this is a SQL Table
- `@PrimaryKey` sets the SQL primary key. All models require a primary key.
- `@POST` reveals the method as an API endpoint with the `POST` HTTP Verb.
- All Cloesce models need to be under a `.cloesce.ts` file.

To compile this model into a working full stack application, Cloesce must undergo both **compilation** and **migrations**. Compilation is the process of extracting the metadata language that powers Cloesce, ensuring it is a valid program, and then producing code to orchestrate the program across different domains (database, backend, frontend, cloud). Migrations utilize the history of validated metadata to create SQL code, translating the evolution of your models.

To compile, run `npx cloesce compile`.

To create a migration, run `npx cloesce migrate <name>`.

After running the above commands, you will have a full project capable of being ran with:

```sh
# Apply the generated migrations
npx wrangler d1 migrations apply <db-name>

# Run the backend
npx wrangler dev
```

Note the output in the `.generated/` dir. These values should not be committed to git, as they depend on the file system of the machine running it.

- `client.ts` is an importable API with all of your backend types and endpoints
- `workers.ts` is the workers entrypoint.
- `cidl.json` is the working metadata for the project

Note the output in `migrations`, ex after running `npx cloesce migrate Initial`

- `<date>_Initial.json` contains all model information necessary from SQL
- `<date>_Initial.sql` contains the acual SQL migrations. In this early version of Cloesce, it's important to check migrations every time.

#### Supported Column Types

Model columns must directly map to SQLite columns. The supported TypeScript types are:

- `number` => `Real` not null
- `string` => `Text` not null
- `boolean` and `Boolean` => `Integer` not null
- `Integer` => `Integer` not null
- `Date` => `Text` (ISO formatted) not null
- `N | null` => `N` (nullable)

Blob types will be added in v0.0.5 when R2 support is added.

## Features

### Wrangler Environment

In order to interact with your database, you will need to define a WranglerEnv

```ts
import { WranglerEnv } from "cloesce/backend";

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
    await env.db.prepare(...);

    return `i am ${this.name}, this is my horse noise`;
  }
}
```

### Foreign Keys, One to One, Data Sources

Complex model relationships are permitted via the `@ForeignKey`, `@OneToOne / @OneToMany @ManyToMany` and `@DataSource` decorators.
Foreign keys are scalar attributes which must reference some other model's primary key:

```ts
@D1
export class Dog {
  @PrimaryKey
  id: number;
}

@D1
export class Person {
  @PrimaryKey
  id: number;

  @ForeignKey(Dog)
  dogId: number;
}
```

This representation is true to the underlying SQL table: `Person` has a column `dogId` which is a foreign key to `Dog`. Cloesce allows you to actually join these tables together in your model representation:

```ts
@D1
export class Dog {
  @PrimaryKey
  id: number;
}

@D1
export class Person {
  @PrimaryKey
  id: number;

  @ForeignKey(Dog)
  dogId: number;

  @OneToOne("dogId") // references Person.dogId
  dog: Dog | undefined; // This value is a "navigation property", which may or may not exist at runtime
}
```

In `v0.0.4`, there are no defaults, only very explicit decisons. Because of that, navigation properties won't exist at runtime unless you tell them to. Cloesce does this via a `DataSource`, which describes the foreign key dependencies you wish to include. All scalar properties are included by default and cannot be excluded.

```ts
@D1
export class Dog {
  @PrimaryKey
  id: number;
}

@D1
export class Person {
  @PrimaryKey
  id: number;

  @ForeignKey(Dog)
  dogId: number;

  @OneToOne("dogId")
  dog: Dog | undefined;

  @DataSource
  static readonly default: IncludeTree<Person> = {
    dog: {}, // says: on model population, join Persons's Dog
  };
}
```

Data sources are just SQL views and can be invoked in your queries. They are aliased in such a way that its similiar to object properties. The frontend chooses which datasource to use in it's API client (all instantiated methods have an implicit DataSource parameter). `null` is a valid option, meaning no joins will occur.

```ts
@D1
export class Person {
  @PrimaryKey
  id: number;

  @ForeignKey(Dog)
  dogId: number;

  @OneToOne("dogId")
  dog: Dog | undefined;

  @DataSource
  static readonly default: IncludeTree<Person> = {
    dog: {},
  };

  @GET
  static async get(id: number, @Inject env: WranglerEnv): Promise<Person> {
    let records = await env.db
      .prepare("SELECT * FROM [Person.default] WHERE [id] = ?") // Person.default is the SQL view generated from the IncludeTree
      .bind(id)
      .run();

    let persons = Orm.mapSql(Person, records.results, Person.default);
    return persons.value[0];
  }
}
```

Note that the `get` code can be simplified using CRUD methods or the ORM primitive `get`.

#### View Aliasing

The generated views will always be aliased so that they can be accessed in an object like notation. For example, given some `Horse` that has a relationship with `Like`:

```ts
@D1
export class Horse {
  @PrimaryKey
  id: Integer;

  name: string;
  bio: string | null;

  @OneToMany("horseId1")
  likes: Like[];

  @DataSource
  static readonly default: IncludeTree<Horse> = {
    likes: { horse2: {} },
  };
}

@D1
export class Like {
  @PrimaryKey
  id: Integer;

  @ForeignKey(Horse)
  horseId1: Integer;

  @ForeignKey(Horse)
  horseId2: Integer;

  @OneToOne("horseId2")
  horse2: Horse | undefined;
}
```

If you wanted to find all horses that like one another, a valid SQL query using the `default` data source would look like:

```sql
SELECT *
FROM [Horse.default]
WHERE
  [likes.horse2.id] = ?
  AND [id] IN (
    SELECT [likes.horse2.id]
    FROM [Horse.default]
    WHERE [id] = ?
  );
```

The actual generated view for `default` looks like:

```sql
CREATE VIEW IF NOT EXISTS "Horse.default" AS
SELECT
    "Horse"."id"          AS "id",
    "Horse"."name"        AS "name",
    "Horse"."bio"         AS "bio",
    "Like"."id"           AS "likes.id",
    "Like"."horseId1"     AS "likes.horseId1",
    "Like"."horseId2"     AS "likes.horseId2",
    "Horse1"."id"         AS "likes.horse2.id",
    "Horse1"."name"       AS "likes.horse2.name",
    "Horse1"."bio"        AS "likes.horse2.bio"
FROM
    "Horse"
LEFT JOIN
    "Like" ON "Horse"."id" = "Like"."horseId1"
LEFT JOIN
    "Horse" AS "Horse1" ON "Like"."horseId2" = "Horse1"."id";
```

#### DataSourceOf<T>

If it is important to determine what data source the frontend called the instantiated method with, the type `DataSourceOf<T>` allows explicit data source parameters:

```ts
@D1
class Foo {
  ...

  @POST
  bar(ds: DataSourceOf<Foo>) {
    // ds = "DataSource1" | "DataSource2" | ... | "none"
  }
}
```

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

  @ManyToMany("StudentsCourses") // same unique id => same jct table.
  students: Student[];
}
```

### ORM Methods

Cloesce provides a suite of ORM methods for getting, listing, updating and inserting models.

#### Upsert

```ts
@D1
class Horse {
  // ...

  @POST
  static async post(@Inject { db }: Env, horse: Horse): Promise<Horse> {
    const orm = Orm.fromD1(db);
    await orm.upsert(Horse, horse, null);
    return (await orm.get(Horse, horse.id, null)).value;
  }
}
```

#### List, Get

```ts
@D1
class Horse {
  // ...
  @GET
  static async get(@Inject { db }: Env, id: number): Promise<Horse> {
    const orm = Orm.fromD1(db);
    return (await orm.get(Horse, id, "default")).value;
  }

  @GET
  static async list(@Inject { db }: Env): Promise<Horse[]> {
    const orm = Orm.fromD1(db);
    return (await orm.list(Horse, "default")).value;
  }
}
```

### CRUD Methods

Generic `GET, POST, PATCH` (and in a future version, DEL) boilerplate methods do not need to be copied around. Cloesce supports CRUD generation, a syntactic sugar that adds the methods to the compiler output.

The `SAVE` method is an `upsert`, meaning it both inserts and updates in the same query.

```ts
@CRUD(["SAVE", "GET", "LIST"])
@D1
export class CrudHaver {
  @PrimaryKey
  id: number;
  name: string;
}
```

which will generate client API methods like:

```ts
static async get(
      id: number,
      dataSource: "none" = "none",
): Promise<HttpResult<CrudHaver>>
```

### Middleware

Cloesce supports middleware at the global level (before routing to a model+method), the model level (before hydration) and the method level (before hydration). Middleware also exposes read/write access to the dependency injection instance that all models use.

Middleware is capable of exiting from the Cloesce Router early with an HTTP Result.

An example of all levels of middleware is below. All middleware must be defined in the file `app.cloesce.ts`.

```ts
@PlainOldObject
export class InjectedThing {
  value: string;
}

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
@CRUD(["POST"])
export class Model {
  @PrimaryKey
  id: number;

  @GET
  static blockedMethod() {}

  @GET
  static getInjectedThing(@Inject thing: InjectedThing): InjectedThing {
    return thing;
  }
}

// Middleware instance
const app: CloesceApp = new CloesceApp();

app.useGlobal((request: Request, env, ir) => {
  if (request.method === "POST") {
    return { ok: false, status: 401, message: "POST methods aren't allowed." };
  }
});

app.useModel(Model, (request, env, ir) => {
  ir.set(InjectedThing.name, {
    value: "hello world",
  });
});

app.useMethod(Model, "blockedMethod", (request, env, ir) => {
  return { ok: false, status: 401, message: "Blocked method" };
});

// Exporting the instance is required
export default app;
```

With this middleware, all POST methods will be blocked, and all methods for the model `Model` will be able to inject `InjectedThing`. Additionally, on the method level, `blockedMethod` will return a 401.

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
    query(): CatStuff {
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

- `src/frontend/ts` run `npm test`
- `src/generator` run `cargo test`

## Integration Tests

- Regression tests: `cargo run --bin test regression`
- Pass fail extractor tests: `cargo run --bin test run-fail`

Optionally, pass `--check` if new snapshots should not be created.

To update integration snapshots, run:

- `cargo run --bin update`

To delete any generated snapshots run:

- `cargo run --bin update -- -d`

## E2E

- `tests/e2e` run `npm test`

## Code Formatting

- `cargo fmt`, `cargo clippy`, `npm run format:fix`
