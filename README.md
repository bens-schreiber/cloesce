# cloesce (unstable, `v0.0.4`)

Cloesce is a full stack compiler for the Cloudflare developer platform, allowing class definitions in high level languages to serve as a metadata basis to create a database schema, backend REST API, frontend API client, and Cloudflare infrastructure (as of v0.0.4, D1 + Workers).

Cloesce is working towards a stable alpha MVP (v0.1.0), with the general milestones being [here](https://cloesce.pages.dev/schreiber/v0.1.0_milestones/).

Internal documentation going over design decisions and general thoughts for each milestone can be found [here](https://cloesce.pages.dev/).

# Documentation

## Getting Started

`v0.0.4` supports only Typescript-to-Typescript projects. An example project is shown [here](https://github.com/bens-schreiber/cloesce/tree/main/examples).

### 1) NPM

Create an NPM project and install cloesce

```sh
npm i cloesce@0.0.4-unstable.8
```

### 2) TypeScript

Create a `tsconfig.json` with the following values:

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

### 3) Cloesce Config

Create a `cloesce.config.json` with your desired configuration:

```json
{
  "source": "./src",
  "workersUrl": "http://localhost:5000/api",
  "clientUrl": "http://localhost:5173/api"
}
```

### 4) Vite

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

Middleware support for CORS is also supported (see Middleware section).

### 5) Wrangler Config

Cloesce will generate any missing `wrangler.toml` values (or the file if missing). A minimal `wrangler.toml` looks like this:

```toml
compatibility_date = "2025-10-02"
main = ".generated/workers.ts"
name = "example"

[[d1_databases]]
binding = "db"
database_id = "..."
database_name = "example"
```

## Cloesce Models

A model is a type which represents:

- a database table,
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

To compile this model into a working full stack application, Cloesce must undergo both **compilation** and **migrations**.

Compilation is the process of extracting the metadata language that powers Cloesce, ensuring it is a valid program, and then producing code to orchestrate the program across different domains (database, backend, frontend, cloud).

Migrations utilize the history of validated metadata to create SQL code, translating the evolution of your models.

### Compiling

- `npx cloesce compile`
- `npx cloesce migrate <name>`.

After running the above commands, you will have a full project capable of being ran with Wrangler:

```sh
# Apply the generated migrations
npx wrangler d1 migrations apply <db-name>

# Run the backend
npx wrangler dev
```

### Compiled Artifacts

#### `.generated/`

These values should not be committed to git, as they depend on the file system of the machine running it.

- `client.ts` is an importable API with all of your backend types and endpoints
- `workers.ts` is the workers entrypoint.
- `cidl.json` is the working metadata for the project

#### `migrations`

After running `npx cloesce migrate <name>`, a new migration will be created in the `migrations/` folder. For example, after creating a migration called `Initial`, you will see:

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
export class MyEnv {
  db: D1Database; // only one DB is supported for now-- make sure it matches the name in `wrangler.toml`

  // you can also define values in the toml under [[vars]]
  motd: string;
}
```

Your WranglerEnv can then be injected into any model method using the `@Inject` decorator:

```ts
@D1
export class Horse {
  @PrimaryKey
  id: number;

  @POST
  async neigh(@Inject env: MyEnv): Promise<string> {
    await env.db.prepare(...);

    return `i am ${this.name}, this is my horse noise`;
  }
}
```

### Foreign Key Column

Reference another model via a foreign key using the `@ForeignKey` decorator:

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

### One to One

Cloesce allows you to relate models via `1:1` relationships using the `@OneToOne` decorator. It requires that a foreign key already exists on the model.

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

In `v0.0.4`, there are no defaults, only very explicit decisons. Because of that, navigation properties won't exist at runtime unless you tell them to. Cloesce does this via a `Data Source`, which describes the foreign key dependencies you wish to include. All scalar properties are included by default and cannot be excluded.

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

Data sources describe how foreign keys should be joined on model hydration (i.e. when invoking any instantiated method). They are composed of an `IncludeTree<T>`, a recursive type composed of the relationships you wish to include. All scalar properties are always included.

Note that `DataSourceOf` is added implicitly to all instantiated methods if no data source parameter is defined.

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

### DataSourceOf<T>

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

Data sources are implicitly added to all instantiated methods if no data source parameter is defined.

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

Both methods take an optional `IncludeTree<T>` parameter to specify what relationships in the generated CTE.

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
}
```

Running `Orm.listQuery` with the data source `Person.default` would produce the CTE:

```sql
WITH "Person_view" AS (
  SELECT
      "Person"."id"      AS "id",
      "Person"."dogId"   AS "dogId",
      "Dog"."id"         AS "dog.id"
  FROM
      "Person"
  LEFT JOIN
      "Dog" ON "Person"."dogId" = "Dog"."id"
) SELECT * FROM "Person_view"
```

Example usages:

```ts
@D1
class Horse {
  // ...
  @GET
  static async get(@Inject { db }: Env, id: number): Promise<Horse> {
    const orm = Orm.fromD1(db);
    return (await orm.get(Horse, id, Horse.default)).value;
  }

  @GET
  static async list(@Inject { db }: Env): Promise<Horse[]> {
    const orm = Orm.fromD1(db);
    return (await orm.list(Horse, {})).value;
  }
}
```

`list` takes an optional `from` parameter to modify the source of the list query. This is useful in filtering / limiting results.

```ts
await orm.list(
  Horse,
  Horse.default,
  "SELECT * FROM Horse ORDER BY name LIMIT 10"
);
```

produces SQL

```sql
WITH "Horse_view" AS (
  SELECT
      "Horse"."id"      AS "id",
      "Horse"."name"    AS "name"
  FROM
      (SELECT * FROM Horse ORDER BY name LIMIT 10) as "Horse"
) SELECT * FROM "Horse_view"
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

Cloesce supports middleware at the global level (before routing to a model+method), the model level (before validation) and the method level (before hydration). Middleware also exposes read/write access to the dependency injection instance that all models use.

Middleware is capable of exiting from the Cloesce Router early with an HTTP Result.

An example of all levels of middleware is below. All middleware must be defined in the file `app.cloesce.ts` which exports a `CloesceApp` instance as default.

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

const app: CloesceApp = new CloesceApp();

app.onRequest((request: Request, env, ir) => {
  if (request.method === "POST") {
    return { ok: false, status: 401, message: "POST methods aren't allowed." };
  }
});

app.onModel(Model, (request, env, ir) => {
  ir.set(InjectedThing.name, {
    value: "hello world",
  });
});

app.onMethod(Model, "blockedMethod", (request, env, ir) => {
  return { ok: false, status: 401, message: "Blocked method" };
});

app.onResponse(async (request, env, di, response: Response) => {
  // basic CORS, allow all origins
  response.headers.set("Access-Control-Allow-Origin", "*");
  response.headers.set(
    "Access-Control-Allow-Methods",
    "GET, POST, PUT, DELETE, OPTIONS"
  );
  response.headers.set(
    "Access-Control-Allow-Headers",
    "Content-Type, Authorization"
  );
});

export default app;
```

With this middleware, all POST methods will be blocked, and all methods for the model `Model` will be able to inject `InjectedThing`,and `blockedMethod` will return a 401. Additionally, all responses will have CORS headers.

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

Optionally, pass `--check` if new snapshots should not be created.

To target a specific fixture, pass `--fixture folder_name`

To update integration snapshots, run:

- `cargo run --bin update`

To delete any generated snapshots run:

- `cargo run --bin update -- -d`

## E2E

- `tests/e2e` run `npm test`

## Code Formatting

- `cargo fmt`, `cargo clippy`, `npm run format:fix`
