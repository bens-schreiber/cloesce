import { D1Database } from "@cloudflare/workers-types";
import {
  Model,
  GET,
  POST,
  PrimaryKey,
  OneToMany,
  OneToOne,
  ForeignKey,
  IncludeTree,
  DataSource,
  Orm,
  WranglerEnv,
  Inject,
} from "cloesce/backend";
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
  motd: string;
}

@Model
class Horse {
  @PrimaryKey
  id: Integer;

  name: string;
  bio: string | null;

  @OneToMany("horseId1")
  likes: Like[];

  @DataSource
  static readonly default: IncludeTree<Horse> = {
    likes: { horse2: {} },
  };

  @DataSource
  static readonly withLikes: IncludeTree<Horse> = {
    likes: {},
  };

  @POST
  static async post(@Inject env: Env, horse: Horse): Promise<Horse> {
    const orm = Orm.fromEnv(env);
    await orm.upsert(Horse, horse, null);
    return (await orm.get(Horse, horse.id, null)).value;
  }

  @GET
  static async get(@Inject env: Env, id: Integer): Promise<Horse> {
    const orm = Orm.fromEnv(env);
    return (await orm.get(Horse, id, Horse.default)).value;
  }

  @GET
  static async list(@Inject env: Env): Promise<Horse[]> {
    const orm = Orm.fromEnv(env);
    return (await orm.list(Horse, Horse.default)).value;
  }

  @POST
  async like(@Inject env: Env, horse: Horse) {
    const orm = Orm.fromEnv(env);
    await orm.upsert(
      Like,
      {
        horseId1: this.id,
        horseId2: horse.id,
      },
      null
    );
  }
}

@Model
class Like {
  @PrimaryKey
  id: Integer;

  @ForeignKey(Horse)
  horseId1: Integer;

  @ForeignKey(Horse)
  horseId2: Integer;

  @OneToOne("horseId2")
  horse2: Horse | undefined;
}

export { Like, Horse };
