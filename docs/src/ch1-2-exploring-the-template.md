# Exploring the Template

After creating your project with `create-cloesce`, several example files are included to help you get started. Below is an overview of those files and their purpose.

## Schema

### Wrangler Environment

All Cloudflare Workers define [a set of bindings](https://developers.cloudflare.com/workers/configuration/environment-variables/) that provision resources such as [D1 databases](https://developers.cloudflare.com/d1/), [R2 buckets](https://developers.cloudflare.com/r2/), [KV namespaces](https://developers.cloudflare.com/kv/concepts/kv-namespaces/), and miscellaneous environment variables.

```cloesce
env {
    d1 { db }
    r2 { bucket }
}
```

In the above Cloesce snippet, we define a Wrangler Environment with a D1 database binding named `db` and an R2 bucket named `bucket`.

After compilation, Cloesce generates a `wrangler.jsonc` (or `wrangler.toml` if configured) with the appropriate bindings for your application based on the `env` block in your schema. Cloesce does not handle provisioning of these resources, so you must assign each resources id to an existing resource in your Cloudflare account.

Read more about the Wrangler Environment in the [Wrangler Environment](./ch2-7-wrangler-environment.md) chapter.

### Models

Models are the core building blocks of a Cloesce application. They define exactly how your data is structured, what relationships exist between different data entities, and what API endpoints will be generated to interact with that data.

Unlike other ORMs, Cloesce Models are not limited to just relational data stored in a SQL database. Models can also include data stored in R2 buckets, KV namespaces, or inject external services.

In `schema/schema.clo` you will find two example Models, `Weather` and `WeatherReport`.

The `Weather` Model consists of:

| Feature         | Type / Description                             | Source / Layer |
| --------------- | ---------------------------------------------- | -------------- |
| `id`            | Primary Key                                    | D1             |
| `weatherReport` | One-to-One relationship                        | D1             |
| `dateTime`      | Scalar column                                  | D1             |
| `temperature`   | Scalar column                                  | D1             |
| `condition`     | Scalar column                                  | D1             |
| `photo`         | R2 object, key format `weather/photo/{id}.jpg` | R2             |
| `uploadPhoto`   | API endpoint                                   | Workers        |
| `downloadPhoto` | API endpoint                                   | Workers        |

The `WeatherReport` Model consists of:
| Feature | Type / Description | Source / Layer |
|---------|-----------------|----------------|
| `id` | Primary Key | D1 |
| `title` | Scalar column | D1 |
| `summary` | Scalar column | D1 |
| `weatherEntries` | One-to-Many relationship with `Weather` | D1 |
| `get` | Generated CRUD operation | Workers |
| `save` | Generated CRUD operation | Workers |
| `list` | Generated CRUD operation | Workers |

<details>
    <summary>Weather Code Snippet</summary>

```cloesce
[use db]
[use list, save, get]
model WeatherReport {
    primary {
        id: int
    }

    nav (Weather::weatherReportId) {
        weatherEntries
    }

    title: string
    description: string
}

```

</details>

<details>
    <summary>WeatherReport Code Snippet</summary>

```cloesce
[use db]
[use get, list, save]
model Weather {
    primary {
        id: int
    }

    foreign (WeatherReport::id) {
        weatherReportId
        nav { weatherReport }
    }

    r2 (bucket, "weather/photos/{id}.jpg") {
        photo
    }

    dateTime: date
    location: string
    temperature: int
    condition: string
}

api Weather {
    post uploadPhoto(self, e: env, s: stream)
    get downloadPhoto([source R2Only] self) -> stream
}

source R2Only for Weather {
    include { photo }
}
```

</details>

Read more about how Models work in the [Models](./ch2-0-models.md) chapter.

## Backend Implementation

In `src/api/main.ts`, you will find all of the TypeScript API route handlers for the example application. These handlers extend generated API interfaces, and must be explicitly registered during application initialization.

While Cloesce has a default Workers entrypoint in the generated backend code, almost every application will require custom API route handlers to implement business logic that cannot be expressed in the schema.

```ts
export const Weather = clo.Weather.impl({
  async uploadPhoto(self, e, s: CfReadableStream) {
    const key = this.Key.photo(self.id);
    await e.bucket.put(key, s);
  },

  downloadPhoto(self) {
    if (!self.photo) {
      return HttpResult.fail(404, "Photo not found");
    }
    return HttpResult.ok(200, self.photo.body);
  },
});

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = (await clo.cloesce()).register(Weather);

    return await app.run(request, env);
  },
};
```
