# Middleware

Middleware functions are events called between states of the Cloesce Router processing pipeline. They can be used to modify requests, exit early, or perform actions before and after operations.

It is important to note that the Cloesce client expects results to come exactly as described in API methods. Therefore, middleware should not modify the structure of a response or change the expected output of an API method unless you are fully aware of the implications.

## Middleware Hooks

> [!WARNING]
> Middleware hooks are likely to change significantly before a stable release.

> [!TIP]
> Many hooks can be registered. Hooks are called in the order they are registered, per hook.

Middleware hooks can be registered to run at specific points in the Cloesce Router processing pipeline. The available middleware hooks are:

| Hook         | Description |
|-------------|-------------|
| `onRoute`    | Called when a request hits a valid route with the correct HTTP method. Service initialization occurs directly after this point, therefore services will not be available. |
| `onNamespace` | Called when a request hits a specific namespace (Model or Service). Occurs after service initialization but before request body validation. |
| `onMethod`| Called when a request is about to invoke a specific method. Occurs after request body validation but before hydration and method execution. |


Each hook has access to the dependency injection container for the current request, allowing you to modify it as needed.

```typescript
export default {
    async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
        if (request.method === "POST") {
            return HttpResult.fail(401, "POST methods aren't allowed.").toResponse();
        }

        const app = await Cloesce.cloesce();
        app.register(new Foo());

        app.onNamespace(Cloesce.Foo.Tag, (di) => {
            di.set(new InjectedThing("hello world"));
        })

        app.onMethod(Cloesce.Foo.Tag, "blockedMethod", (_di) => {
            return HttpResult.fail(401, "Blocked method");
        });

        return await app.run(request, env);
    }
}
```

Middleware is capable of short-circuiting the request processing by returning an `HttpResult` directly. This is useful for implementing features like authentication. Middleware can also modify the dependency injection container for the current request, allowing you to inject custom services or data.