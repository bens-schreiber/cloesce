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
    return horse;
  }

  @GET
  static async get(@Inject { db }: Env, id: number): Promise<Horse> {
    let records = await db
      .prepare("SELECT * FROM [Horse.default] WHERE [Horse.id] = ?")
      .bind(id)
      .run();

    const res = Orm.fromSql(Horse, records.results, Horse.default);
    return res.value[0];
  }

  @GET
  static async list(@Inject { db }: Env): Promise<Horse[]> {
    let records = await db.prepare("SELECT * FROM [Horse.default]").run();

    const res = Orm.fromSql(Horse, records.results, Horse.default);
    return res.value;
  }

  @POST
  async like(@Inject { db }: Env, horse: Horse) {
    // TODO: Revisit this. If we wanted to use `Orm.insert` we'd have to specify
    // the Like id, when we'd rather have that auto generate.
    await db
      .prepare("INSERT INTO Like (horseId1, horseId2) VALUES (?, ?)")
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
