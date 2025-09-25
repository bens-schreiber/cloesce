import { D1Database } from "@cloudflare/workers-types";
import {
  D1,
  GET,
  PATCH,
  POST,
  PrimaryKey,
  OneToMany,
  OneToOne,
  ForeignKey,
  Result,
  IncludeTree,
  DataSource,
  modelsFromSql,
} from "cloesce";
import cidl from "../../.generated/cidl.json" with { type: "json" };

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

  @POST
  static async post(db: D1Database, horse: Horse): Promise<Result<Horse>> {
    const records = await db
      .prepare("INSERT INTO Horse (id, name, bio) VALUES (?, ?, ?) RETURNING *")
      .bind(horse.id, horse.name, horse.bio)
      .all();

    let horseRes = modelsFromSql<Horse>(
      "Horse",
      cidl,
      records.results,
      Horse.default
    )[0];

    return { ok: true, status: 200, data: horseRes };
  }

  @GET
  static async get(db: D1Database, id: number): Promise<Result<Horse>> {
    let records = await db
      .prepare("SELECT * FROM Horse_default WHERE Horse_id = ?")
      .bind(id)
      .run();
    console.log(records.results);
    let horses = modelsFromSql<Horse>(
      "Horse",
      cidl,
      records.results,
      Horse.default
    );
    console.log(JSON.stringify(horses));
    return { ok: true, status: 200, data: horses[0] };
  }

  @GET
  static async list(db: D1Database): Promise<Result<Horse[]>> {
    let records = await db.prepare("SELECT * FROM Horse_default").run();
    let horses = modelsFromSql<Horse>(
      "Horse",
      cidl,
      records.results,
      Horse.default
    );
    return { ok: true, status: 200, data: horses };
  }

  @PATCH
  async patch(db: D1Database, horse: Horse): Promise<Result<void>> {
    await db
      .prepare("UPDATE Horse SET name = ?, bio = ? WHERE Horse.id = ?")
      .bind(horse.name, horse.bio, horse.id)
      .run();
    return { ok: true, status: 200 };
  }

  @POST
  async like(db: D1Database, horse: Horse): Promise<Result<void>> {
    await db
      .prepare("INSERT INTO Like (horseId1, horseId2) VALUES (?, ?)")
      .bind(this.id, horse.id)
      .run();
    return { ok: true, status: 200 };
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
