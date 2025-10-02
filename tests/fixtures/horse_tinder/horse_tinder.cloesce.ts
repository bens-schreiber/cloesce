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
  modelsFromSql,
  WranglerEnv,
} from "cloesce";

@WranglerEnv
class Env {
  D1_DB: D1Database;
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
  static async post(@Inject { D1_DB }: Env, horse: Horse): Promise<Horse> {
    const records = await D1_DB.prepare(
      "INSERT INTO Horse (id, name, bio) VALUES (?, ?, ?) RETURNING *"
    )
      .bind(horse.id, horse.name, horse.bio)
      .all();

    return modelsFromSql(Horse, records.results, Horse.default)[0] as Horse;
  }

  @GET
  static async get(@Inject { D1_DB }: Env, id: number): Promise<Horse> {
    let records = await D1_DB.prepare(
      "SELECT * FROM Horse_default WHERE Horse_id = ?"
    )
      .bind(id)
      .run();

    return modelsFromSql(Horse, records.results, Horse.default)[0] as Horse;
  }

  @GET
  static async list(@Inject { D1_DB }: Env): Promise<Horse[]> {
    let records = await D1_DB.prepare("SELECT * FROM Horse_default").run();
    return modelsFromSql(Horse, records.results, Horse.default) as Horse[];
  }

  @POST
  async like(@Inject { D1_DB }: Env, horse: Horse) {
    await D1_DB.prepare("INSERT INTO Like (horseId1, horseId2) VALUES (?, ?)")
      .bind(this.id, horse.id)
      .run();
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
