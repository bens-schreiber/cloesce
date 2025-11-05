// GENERATED CODE. DO NOT MODIFY.
import { cloesce, CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Foo } from "./seed__ds.cloesce.ts";
import { NoDs } from "./seed__ds.cloesce.ts";
import { OneDs } from "./seed__ds.cloesce.ts";
import { Poo } from "./seed__ds.cloesce.ts";
const app = new CloesceApp();
const constructorRegistry = {
  Foo: Foo,
  NoDs: NoDs,
  OneDs: OneDs,
  Poo: Poo,
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
  try {
    const envMeta = { envName: "Env", dbName: "db" };
    const apiRoute = "/api";
    return await cloesce(
      request,
      env,
      cidl,
      app,
      constructorRegistry,
      envMeta,
      apiRoute
    );
  } catch (e: any) {
    return new Response(
      JSON.stringify({
        ok: false,
        status: 500,
        message: e.toString(),
      }),
      {
        status: 500,
        headers: { "Content-Type": "application/json" },
      }
    );
  }
}

export default { fetch };
