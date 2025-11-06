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
@CRUD(["GET", "LIST", "POST"])
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

    const res = Orm.fromSql(Horse, records.results, Horse.withLikes);
    if (res.ok) {
      return res.value;
    }

    return [];
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
