import { D1Database } from "@cloudflare/workers-types";
import {
  Model,
  POST,
  GET,
  Inject,
  PrimaryKey,
  Orm,
  WranglerEnv,
  DeepPartial,
} from "cloesce/backend";
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model
export class Dog {
  id: Integer;

  name: string;
  age: Integer;

  @POST
  static async post(@Inject env: Env, dog: DeepPartial<Dog>): Promise<Dog> {
    const orm = Orm.fromEnv(env);
    return await orm.upsert(Dog, dog, null);
  }

  @GET
  getPartialSelf(): DeepPartial<Dog> {
    return {
      name: this.name,
    }
  }
}
