# APIs

Cloesce generates a RPC-like REST API for every Model in your application. APIs defined in the schema become methods on a generated class, which can be called from a remote client transparently as if the object existed on the client.

Every Data Source defined in your schema becomes an access point for the client to retrieve and update Models.

This chapter covers how to:

- Define [REST APIs](./ch6-1-rest-apis.md)
- Utilize generated [CRUD APIs](./ch6-2-crud-generation.md)
- Inject [dependencies](./ch6-3-dependency-injection.md) into APIs
- Validate API inputs with [Runtime Validation](./ch6-4-runtime-validation.md)
- Define [Services](./ch6-6-services.md) and [Plain Old Objects](./ch6-5-plain-old-objects.md)
