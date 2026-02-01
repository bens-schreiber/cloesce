# Middleware

Middleware functions are events called in between states of the Cloesce Router processing pipeline. They can be used to modify requests, exit early, or perform actions before or after certain operations.

It is important to note that the Cloesce client expects results to come exactly as they have been described in API methods. Therefore, middleware should not modify the structure of a response or change the expected output of an API method unless you are fully aware of the implications.

## Custom Main Entrypoint

The most basic form of middleware is a custom main entrypoint function, which will be called for every request to your Cloesce application.

Cloesce will search your project for an exported `main` entrypoint. If it doesn't appear, a default main will be generated that simply initializes the Cloesce application. The main entrypoint allows you to intercept a request before it reaches the Cloesce Router, and handle the output of the Cloesce Router as you see fit.

Below is an example of using the main entrypoint to attach CORS headers to every response:

```typescript
import { CloesceApp } from "cloesce/backend";
export default async function main(
    request: Request,
    env: Env,
    app: CloesceApp,
    _ctx: ExecutionContext): Promise<Response> {
    // preflight
    if (request.method === "OPTIONS") {
        return HttpResult.ok(200, undefined, {
            "Access-Control-Allow-Origin": "*",
            "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
            "Access-Control-Allow-Headers": "Content-Type, Authorization",
        }).toResponse();
    }

    // Run Cloesce router
    const result = await app.run(request, env);

    // attach CORS headers
    result.headers.set("Access-Control-Allow-Origin", "*");
    result.headers.set(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, OPTIONS"
    );
    result.headers.set(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization"
    );

    return result;
}
```

> *Note*: The Cloesce Router will never throw an unhandled exception. All errors are converted into `HttpResult` responses. Therefore, there is no need to wrap `app.run` in a try/catch block. 500 errors are logged by default.

## Middleware Hooks

>*Alpha Note*: Middleware hooks are likely to change significantly before a stable release.

Middleware hooks can be registered to run at specific points in the Cloesce Router processing pipeline. The available middleware hooks are:

| Hook         | Description |
|-------------|-------------|
| `onRoute`    | Called when a request hits a valid route with the correct HTTP method. Service initialization occurs directly after this point, thus services will not be available. |
| `onNamespace` | Called when a request hits a specific namespace (Model or Service). Occurs after service initialization but before request body validation. |
| `onMethod`| Called when a request is about to invoke a specific method. Occurs after request body validation but before hydration and method execution. |


> *Note*: Many hooks can be registered. Hooks are called in the order they are registered, per hook.

Each hook has access to the dependency injection container for the current request, allowing you to modify it as needed.

```typescript

export class InjectedThing {
    value: string;
}

export default async function main(
    request: Request,
    env: Env,
    app: CloesceApp,
    _ctx: ExecutionContext): Promise<Response> {
        app.onNamespace(Foo, (di) => {
            di.set(InjectedThing, {
                value: "hello world",
            });
        });

        app.onMethod(Foo, "blockedMethod", (_di) => {
            return HttpResult.fail(401, "Blocked method");
        });

        return await app.run(request, env);
}
```

Middleware is capable of short-circuiting the request processing by returning an `HttpResult` directly. This is useful for implementing features like authentication. Middleware can also modify the dependency injection container for the current request, allowing you to inject custom services or data.

> *Alpha Note*: Middleware can only inject classes into the DI container at this time. Injecting primitive values (strings, numbers, etc) is not yet supported, but a solution is planned for a future release.