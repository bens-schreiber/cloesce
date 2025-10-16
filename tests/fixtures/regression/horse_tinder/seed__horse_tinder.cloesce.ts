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
  motd: string;
}

@D1
class Horse {
  @PrimaryKey
  id: number;

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
  static async post(@Inject { db }: Env, horse: Horse): Promise<Horse> {
    const orm = Orm.fromD1(db);
    await orm.insert(Horse, horse, null);
    return (await orm.get(Horse, horse.id, null)).value;
  }

  @GET
  static async get(@Inject { db }: Env, id: number): Promise<Horse> {
    const orm = Orm.fromD1(db);
    return (await orm.get(Horse, id, "default")).value;
  }

  @GET
  static async list(@Inject { db }: Env): Promise<Horse[]> {
    const orm = Orm.fromD1(db);
    return (await orm.list(Horse, "default")).value;
  }

  @POST
  async like(@Inject { db }: Env, horse: Horse) {
    const orm = Orm.fromD1(db);
    await orm.insert(
      Like,
      {
        horseId1: this.id,
        horseId2: horse.id,
      },
      null
    );
  }
}

@D1
class Like {
  @PrimaryKey
  id: number;

  @ForeignKey(Horse)
  horseId1: number;

  @ForeignKey(Horse)
  horseId2: number;

  @OneToOne("horseId2")
  horse2: Horse | undefined;
}

export { Like, Horse };
