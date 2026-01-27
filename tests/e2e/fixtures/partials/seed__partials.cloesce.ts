import { D1Database } from "@cloudflare/workers-types";
import {
  Model,
  POST,
  GET,
  Inject,
  Orm,
  WranglerEnv,
  DeepPartial,
  Integer,
} from "cloesce/backend";

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model()
export class Dog {
  id: Integer;

  name: string;
  age: Integer;

  @POST
  static async post(@Inject env: Env, dog: DeepPartial<Dog>): Promise<Dog> {
    const orm = Orm.fromEnv(env);
    return (await orm.upsert(Dog, dog, null))!;
  }

  @GET
  getPartialSelf(): DeepPartial<Dog> {
    return {
      name: this.name,
    };
  }
}
