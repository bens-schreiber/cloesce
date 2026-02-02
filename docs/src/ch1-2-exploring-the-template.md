# Exploring the Template

After creating your project with `create-cloesce`, several example files are included to help you get started. Below is an overview of those files and their purpose.

## Wrangler Environment

All Cloudflare Workers define a [a set of bindings](https://developers.cloudflare.com/workers/configuration/environment-variables/) that provision resources such as [D1 databases](https://developers.cloudflare.com/d1/), [R2 buckets](https://developers.cloudflare.com/r2/), [KV namespaces](https://developers.cloudflare.com/kv/concepts/kv-namespaces/), and miscellaneous environment variables.

Cloesce uses a class decorated with `@WranglerEnv` to define the Wrangler Environment for your application, tailoring to the resources you need.

In `src/data/main.ts`, a basic Wrangler Environment has been defined.

```typescript
@WranglerEnv
export class Env {
    db: D1Database;
    bucket: R2Bucket;
    myVariable: string;
}
```

The above implementation of `Env` defines a Wrangler environment with a D1 database binding named `db`, an R2 bucket named `bucket`, and a string environment variable named `myVariable`. 

A typical Cloudflare Worker defines these bindings in a `wrangler.toml` file, but Cloesce generates this file for you during compilation based on the `@WranglerEnv` class.

Read more about the Wrangler Environment in the [Wrangler Environment](./ch2-7-wrangler-environment.md) chapter.

## Custom Main Function

Cloudflare Workers are serverless functions that run at Cloudflare’s edge and respond to HTTP requests. Each Worker defines an entry point function through which all requests are routed.

Cloesce allows this same functionality through a custom `main` definition (seen in  `src/data/main.ts`)

```typescript
export default async function main(
    request: Request,
    env: Env,
    app: CloesceApp,
    ctx: ExecutionContext
): Promise<Response> {...}
```

<!-- Just like a standard Cloudflare Worker, this function receives a `Request`, `Env` and `ExecutionContext` object. Additionally, it receives a `CloesceApp` instance that you can use to handle routing and Model operations. -->

Just like the standard Workers entrypoint, this function receives the inbound `Request`, the Wrangler Environment defined by the decorated `@WranglerEnv` class, and an `ExecutionContext` for managing background tasks.

Additionally, it receives a `CloesceApp` instance that you can use to handle routing and Model operations.

Read more about custom main functions in the [Middleware](./ch4-0-middleware.md) chapter.

## Example Models

Models are the core building blocks of a Cloesce application. They define exactly how your data is structured, what relationships exist between different data entities, and what API endpoints will be generated to interact with that data.

Unlike other ORMs, Cloesce Models are not limited to just relational data stored in a SQL database. Models can also include data stored in R2 buckets, KV namespaces, or inject external services.

In `src/data/Models.ts` you will find two example Models, `Weather` and `WeatherReport`.

<details>
    <summary>Weather Code Snippet</summary>

```typescript
@Model()
export class Weather {
    id: Integer;

    weatherReportId: Integer;
    weatherReport: WeatherReport | undefined;

    dateTime: Date;
    location: string;
    temperature: number;
    condition: string;

    @R2("weather/photo/{id}", "bucket")
    photo: R2ObjectBody | undefined;

    static readonly withPhoto: IncludeTree<Weather> = {
        photo: {}
    }

    @POST
    async uploadPhoto(@Inject env: Env, stream: ReadableStream) {... }

    @GET
    downloadPhoto() {... }
}
```
</details>

<details>
    <summary>WeatherReport Code Snippet</summary>

```typescript
@Model(["GET", "LIST", "SAVE"])
export class WeatherReport {
    id: Integer;

    title: string;
    description: string;

    weatherEntries: Weather[];

    static readonly withWeatherEntries: IncludeTree<WeatherReport> = {
        weatherEntries: {}
    }
}
```
</details>

The `Weather` Model conists of:

| Feature | Type / Description | Source / Layer |
|---------|-----------------|----------------|
| `id` | Primary Key | D1 |
| `weatherReport` | One-to-One relationship | D1 |
| `dateTime` | Scalar column | D1 |
| `temperature` | Scalar column | D1 |
| `condition` | Scalar column | D1 |
| `photo` | R2 object, key format `weather/photo/{id}` | R2 |
| `withPhoto` | IncludeTree | Cloesce |
| `uploadPhoto` | API endpoint | Workers |
| `downloadPhoto` | API endpoint | Workers |



The `WeatherReport` Model consists of:
| Feature | Type / Description | Source / Layer |
|---------|-----------------|----------------|
| `id` | Primary Key | D1 |
| `title` | Scalar column | D1 |
| `summary` | Scalar column | D1 |
| `weatherEntries` | One-to-Many relationship with `Weather` | D1 |
| `withWeatherEntries` | IncludeTree | Cloesce |
| `GET` | Generated CRUD operation | Workers |
| `SAVE` | Generated CRUD operation | Workers |
| `LIST` | Generated CRUD operation | Workers |

Read more about how models work in the [Models](./ch2-0-models.md) chapter.