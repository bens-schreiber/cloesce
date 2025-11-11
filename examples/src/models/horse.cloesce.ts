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
  WranglerEnv,
  Orm,
  CRUD,
  Integer,
} from "cloesce/backend";

@WranglerEnv
export class Env {
  db: D1Database;
  motd: string;
}

@D1
@CRUD(["GET", "LIST", "SAVE"])
export class Horse {
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
  async like(@Inject { db }: Env, horse: Horse) {
    const orm = Orm.fromD1(db);
    await orm.upsert(Like, {
      horseId1: this.id,
      horseId2: horse.id,
    });
  }

  @GET
  async matches(@Inject { db }: Env): Promise<Horse[]> {
    const sql = Orm.listQuery(Horse, {
      includeTree: Horse.default,
      tagCte: "Horse.default",
    })
      .map(
        (q) => `
        ${q}
        WHERE
          [likes.horse2.id] = ?
          AND [id] IN (
            SELECT [likes.horse2.id]
            FROM [Horse.default]
            WHERE [id] = ?
          )
      `
      )
      .unwrap();

    const records = await db.prepare(sql).bind(this.id, this.id).run();
    const res = Orm.mapSql(Horse, records.results, Horse.withLikes);

    return res.mapLeft((_) => []).unwrap();
  }
}

@D1
export class Like {
  @PrimaryKey
  id: Integer;

  @ForeignKey(Horse)
  horseId1: Integer;

  @ForeignKey(Horse)
  horseId2: Integer;

  @OneToOne("horseId2")
  horse2: Horse | undefined;
}
