# Dependency Injection

Any API method may optionally inject [Environment Bindings](./ch3-0-environment.md), or define custom object bindings to inject.

This allows you to easily access resources such as D1 databases, KV namespaces, R2 buckets, and more within your API implementations without needing a globally scoped environment object.

## Injecting Environment Bindings

To inject an Environment Binding, add the `inject` tag to the API method and specify the name of the binding you want to inject:

```cloesce
env {
    d1 {
        db
    }

    r2 {
        bucket
    }

    vars {
        secret_value: string
    }
}

model Person {
    primary {
        id: int
    }
}

api Person {
    [inject db, bucket, secret_value]
    get do_stuff(self) -> Person
}
```

In the above code, the backend stub generated for the `do_stuff` API method will pass in a parameter `env` of the type:

```ts
{
  db: D1Database;
  bucket: R2Bucket;
  secret_value: string;
}
```

## Defining Custom Inject Bindings

In addition to Environment Bindings, you can also define your own custom Inject bindings. This is useful for cases where you want to inject a resource that is not part of the environment, or if you want to perform some custom logic before injecting the resource.

To define some custom structure to be injected, you can use the `inject` keyword in your API definition:

```cloesce
inject {
    YouTubeApi
    OpenAiClient
}

// ... and then inject as usual:
api Person {
    [inject YouTubeApi, OpenAiClient]
    get do_stuff(self) -> Person
}
```

Unlike Environment Bindings, custom Injections require an explicit implementation:

```ts
import * as clo from "@cloesce/backend.js";

class YouTubeApi extends clo.YouTubeApi {
  // Add custom methods or properties!
  constructor() {
    super();
  }
}

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = await clo.cloesce();
    app.register(new YouTubeApi());

    return await app.run(request, env);
  },
};
```
