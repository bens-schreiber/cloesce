import { D1Database } from "@cloudflare/workers-types";
import {
  D1,
  GET,
  PATCH,
  POST,
  Inject,
  PrimaryKey,
  OneToMany,
  OneToOne,
  ForeignKey,
  IncludeTree,
  DataSource,
  Orm,
  WranglerEnv,
} from "cloesce/backend";

@WranglerEnv
class Env {
  db: D1Database;
}

@D1
export class Dog {
  @PrimaryKey
  id: number;

  name: string;
  age: number;

  @POST
  static async post(@Inject { db }: Env, dog: Partial<Dog>): Promise<Dog> {
    const orm = Orm.fromD1(db);
    const res = await orm.insert(Dog, dog, null);
    return (await orm.get(Dog, res.value, null)).value;
  }
}
