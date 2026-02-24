import { D1Database } from "@cloudflare/workers-types";
import {
  Model,
  Post,
  Get,
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

  @Post()
  static async post(@Inject env: Env, dog: DeepPartial<Dog>): Promise<Dog> {
    const orm = Orm.fromEnv(env);
    return (await orm.upsert(Dog, dog))!;
  }

  @Get()
  getPartialSelf(): DeepPartial<Dog> {
    return {
      name: this.name,
    };
  }
}
