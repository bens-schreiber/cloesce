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

Let's create a simple Service that returns a "Hello, World!" message.

```typescript
import { Service, GET, HttpResult } from 'cloesce/backend';

@Service
export class HelloWorldService {

    init(): HttpResult<void> | undefined {
        // Optional initialization logic can go here
    }

    @GET
    hello(): string {
        return "Hello, World!";
    }
}
```

After running `npx cloesce compile`, this Service will be available at the endpoint `HelloWorldService/hello`, and a client method will be generated for you to call it from the frontend.

## Dependency Injection

To share dependencies across your Cloesce application methods, Cloesce utilizes dependency injection. By default, Cloesce provides two dependencies:
- Wrangler Environment: Access to your Cloudflare Workers environment variables.
- Request: The incoming HTTP request object.

You can access these dependencies by decorating your method parameters with the `@Inject` decorator on any Cloesce Model or Service method:
```typescript
import { Service, GET, WranglerEnv } from 'cloesce/backend';

@WranglerEnv
class Env {
    d1: D1Database;
}

@Service
export class HelloWorldService {
    @GET
    async hello(@Inject env: Env, @Inject request: Request): Promise<string> {
        console.log("Request URL:", request.url);
        const res = await env.d1.prepare("SELECT 'Hello, World!' AS message").first<{ message: string }>();
        return res.message;
    }
}
```

Unlike Models, which require all attributes to be SQL columns, KV keys, or R2 objects, Services allow attributes to be any arbitrary value, searching for them in the dependency injection context. This means you can easily inject custom Services, utilities, or configurations into your Service methods as needed.

```typescript
@Service
export class HelloWorldService {

    env: Env;
    request: Request;
    foo: string;

    init(): void {
        this.foo = "bar";
    }

    @GET
    async hello(): Promise<string> {
        console.log("Request URL:", this.request.url);
        const res = await this.env.d1.prepare("SELECT 'Hello, World!' AS message").first<{ message: string }>();
        return res.message + " and foo is " + this.foo;
    }
}
```

## Services as Dependencies

Services can also be injected into other Services. They cannot be circularly dependent, but otherwise, you can freely compose Services together.

```typescript
@Service
export class GreetingService {
    greet(name: string): string {
        return `Hello, ${name}!`;
    }
}

@Service
export class HelloWorldService {
    greetingService: GreetingService;

    @GET
    hello(name: string): string {
        return this.greetingService.greet(name);
    }
}
```
