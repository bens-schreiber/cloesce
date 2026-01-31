# Middleware Basics

Middleware functions are events called in between states of the Cloesce Router processing pipeline. They can be used to modify requests, exit early, or perform actions before or after certain operations.

It is important to note that the Cloesce client expects results to come exactly as they have been described in API methods. Therefore, middleware should not modify the structure of a response or change the expected output of an API method, unless you are fully aware of the implications.

## Custom Main Entrypoint

Cloesce will search your project for an exported `main` entrypoint. If it doesn't appear, a default main will be generated that simply initializes the Cloesce application. The main entrypoint allows you to intercept a request before it reaches the Cloesce Router, and handle the output of the Cloesce Router as you see fit.

Below is an example of using the main entrypoint to attatch CORS headers to every response:

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

## Middleware Functions

Middleware functions can be registered to run at specific points in the Cloesce Router processing pipeline. The available middleware hooks are:
- `onRoute` - Called when a request hits a valid route with the correct HTTP method. Noteably, service initialization occurs after this point.
- `onNamespace` - Called when a request hits a specific namespace (be it a Model or Service). This occurs after service initialization, but before the request body is validated.
- `onMethod` - Called when a request is about to invoke a specific method. This occurs after the request body has been validated, but before hydration and method execution.

> *Note*: Middleware functions are called in the order they are registered, per hook.

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

> *Alpha Note*: Middleware can only inject classes into the DI container at this time. Injecting primitive values (strings, numbers, etc) is not yet supported, but planned for a future release.