# Services

> [!TIP]
> A clean design pattern for Cloesce is to use Services to encapsulate significant business logic and have Models act as
> thin wrappers around data storage and retrieval. 
>
> This separation of concerns can lead to more maintainable and testable code.

Models are not the only way to write API logic in Cloesce. 

Services are another core concept that allows you to encapsulate business logic and share it across your application. Services are similar to Models in that they can define methods that can be called from your API routes, but they do not have any associated data storage or schema. 

Instead, Services are used to group related, complex functionality together and can be injected into other parts of your application using Cloesce's dependency injection system.

## Hello World Service

```cloesce
service HelloWorldService {
    helloString: string
}

api HelloWorldService {
    hello(self) -> string
}
```



After running `cloesce compile`, a backend method can be implemented for the `hello` API:
```typescript
import * as Cloesce from "@cloesce/backend";

class HelloWorldService extends Cloesce.HelloWorldService.Api {
    init(self: Cloesce.HelloWorldService.Self): void {
        self.helloString = "Hello, World!";
    }

    hello(self): string {
        return self.helloString;
    }
}
```

## Dependency Injection

To share dependencies across your Cloesce application methods, Cloesce utilizes a dependency injection container. By default, Cloesce provides only the Wrangler environment as a dependency.

To define a custom dependency, simply add an `inject` block to your schema:
```cloesce
inject {
    MyDependency
}
```

This type can then be passed in to any `api` method or `service` definition and will be resolved by the dependency injection container at runtime. Additionally, typing a field with `env` will inject the Wrangler environment.

```cloesce
env {
    // ...
}

inject {
    YouTubeApiClient
}

service VideoService {
    wrangler: env
    ytClient: YouTubeApiClient
}

api VideoService {
    getVideo(self, videoId: string) -> stream

    // also valid
    getVideoStatic(wrangler: env, ytClient: YouTubeApiClient, videoId: string) -> stream
}
```


## Services as Dependencies

Services can also be injected into other Services. They cannot be circularly dependent, but otherwise, you can freely compose Services together.

```cloesce

service FooService { }

service BarService {
    foo: FooService
}
```
