import { D1Database } from "@cloudflare/workers-types";
import {
  D1,
  GET,
  POST,
  Inject,
  PrimaryKey,
  OneToMany,
  OneToOne,
  ForeignKey,
  IncludeTree,
  DataSource,
  modelsFromSql,
  WranglerEnv,
} from "cloesce";

@WranglerEnv
class Env {
  db: D1Database;
  motd: string;
}

@D1
export class Horse {
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
    const records = await db
      .prepare("INSERT INTO Horse (id, name, bio) VALUES (?, ?, ?) RETURNING *")
      .bind(horse.id, horse.name, horse.bio)
      .all();

    return modelsFromSql(Horse, records.results, Horse.default)[0] as Horse;
  }

  @GET
  static async get(@Inject { db }: Env, id: number): Promise<Horse> {
    let records = await db
      .prepare("SELECT * FROM Horse_default WHERE Horse_id = ?")
      .bind(id)
      .run();

    return modelsFromSql(Horse, records.results, Horse.default)[0] as Horse;
  }

  @GET
  static async list(@Inject { db }: Env): Promise<Horse[]> {
    let records = await db.prepare("SELECT * FROM Horse_default").run();
    return modelsFromSql(Horse, records.results, Horse.default) as Horse[];
  }

  @POST
  async like(@Inject { db }: Env, horse: Horse) {
    await db
      .prepare("INSERT INTO Like (horseId1, horseId2) VALUES (?, ?)")
      .bind(this.id, horse.id)
      .run();
  }
}

@D1
export class Like {
  @PrimaryKey
  id: number;

  @ForeignKey(Horse)
  horseId1: number;

  @ForeignKey(Horse)
  horseId2: number;

  @OneToOne("horseId2")
  horse2: Horse | undefined;
}
