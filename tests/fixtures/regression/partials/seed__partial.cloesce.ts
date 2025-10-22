import { D1Database } from "@cloudflare/workers-types";
import {
  D1,
  POST,
  Inject,
  PrimaryKey,
  Orm,
  WranglerEnv,
  DeepPartial,
} from "cloesce/backend";

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class Dog {
  @PrimaryKey
  id: number;

  name: string;
  age: number;

  @POST
  static async post(@Inject { db }: Env, dog: DeepPartial<Dog>): Promise<Dog> {
    const orm = Orm.fromD1(db);
    const res = await orm.upsert(Dog, dog, null);
    return (await orm.get(Dog, res.value, null)).value;
  }
}
