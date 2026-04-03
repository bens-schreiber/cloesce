// @ts-nocheck


//region API EXAMPLE
class UserApi extends Cloesce.Apis.User {
    route1(self: Cloesce.Models.User, e: Cloesce.Env): HttpResult<string> {
        //...
    }

    async route2(e: Cloesce.Env): HttpResult<string> { }
}

class WeatherApi extends Cloesce.Apis.Weather {
    async upload_photo(self: Cloesce.Models.Weather, e: Cloesce.Env, s: Uint8Array): HttpResult<void> {
        //...
    }
}

import { cloesce } from "./app";

export default async function fetch(request: Request, env: Cloesce.Env, ctx: ExecutionContext): Promise<Response> {

    // Run Cloesce app
    const app = cloesce()
        .register(new UserApi())
        .register(new WeatherApi()); // ... register other APIs

    const result = await app.run(request, env);

    return result;
}
//endregion



// NEW API ROUTES
// instantiated identifier = [data source "get" params, key fields]
// if there is no "get" data source method available, the identifier is just the key fields.
//
// if the API method is static, there is no identifier.
//
// ex instantiated route: "ModelName/{id1}/{id2}/.../{key1}/{key2}/.../MethodName"
//
// ex static route: "ModelName/MethodName"