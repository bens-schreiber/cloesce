# Exploring the Template

After creating your project with `create-cloesce`, several example files are included to help you get started. Below is an overview of these files and their purposes.

## Example Wrangler Env

In `src/data/main.ts`, a basic WranglerEnv has been set up to define your Cloudflare Workers environment. You can modify this file to add your own environment variables, KV namespaces, R2 buckets, and Durable Object bindings.

```typescript
@WranglerEnv
export class Env {
    db: D1Database;
    bucket: R2Bucket;
    myVariable: string;
}
```

The above implementation of `Env` defines a Wrangler environment with a D1 database binding named `db`, an R2 bucket named `bucket`, and a string environment variable named `myVariable`. In the build step, Cloesce will generate a matching `wrangler.toml` file based on this definition.

## Custom Main Function

A custom `main` function which acts as the worker entry point is defined in `src/data/main.ts`.

```typescript
export default async function main(
    request: Request,
    env: Env,
    app: CloesceApp,
    ctx: ExecutionContext
): Promise<Response> {...}
```

Just like a standard Cloudflare Worker, this function receives a `Request`, `Env` and `ExecutionContext` object. Additionally, it receives a `CloesceApp` instance that you can use to handle routing and model operations.

> *TIP*: It is not always necessary to define a custom main function. If you do not need custom logic before Cloesce handles the request, you can omit main entirely and a default implementation will be used.

## Example Models

In `src/data/models.ts` you will find two example models, `Weather` and `WeatherReport`.

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
- A primary key `id`
- A `One to One` relationship with `WeatherReport`
- Four scalar columns: `dateTime`, `temperature`, `condition`
- A `R2` object `photo` which uses the key format `weather/photo/{id}`
- An `IncludeTree` `withPhoto`
- Two API endpoints `uploadPhoto` and `downloadPhoto`


The `WeatherReport` Model consists of:
- Three generated CRUD operations: `GET`, `SAVE`, `LIST`
- A primary key `id`
- Two scalar columns: `title`, `summary`
- A `One to Many` relationship with `Weather`
- An `IncludeTree` `withWeatherEntries`

