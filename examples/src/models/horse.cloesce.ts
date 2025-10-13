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
} from "cloesce/backend";

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
      .prepare("SELECT * FROM [Horse.default] WHERE [Horse.id] = ?")
      .bind(id)
      .run();

    return modelsFromSql(Horse, records.results, Horse.default)[0] as Horse;
  }

  @GET
  static async list(@Inject { db }: Env): Promise<Horse[]> {
    let records = await db.prepare("SELECT * FROM [Horse.default]").run();
    return modelsFromSql(Horse, records.results, Horse.default) as Horse[];
  }

  @POST
  async like(@Inject { db }: Env, horse: Horse) {
    await db
      .prepare("INSERT INTO Like (horseId1, horseId2) VALUES (?, ?)")
      .bind(this.id, horse.id)
      .run();
  }

  @GET
  async matches(@Inject { db }: Env): Promise<Horse[]> {
    const records = await db
      .prepare(
        `
    SELECT * FROM [Horse.default] as H1
    WHERE
        H1.[Horse.id] = ?
        AND EXISTS (
            SELECT 1
            FROM [Horse.default] AS H2
            WHERE H2.[Horse.id] = H1.[Horse.likes.horse2.id]
              AND H2.[Horse.likes.horse2.id] = H1.[Horse.id]
        );
    `
      )
      .bind(this.id)
      .run();

    return modelsFromSql(Horse, records.results, null);
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
