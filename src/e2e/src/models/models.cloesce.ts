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
  mapSql,
} from "cloesce";

@D1
class Horse {
  // Denotes that `id` is the primary key of table `Horse`
  @PrimaryKey
  id: number;

  name: string;
  bio: string | null;

  // A navigation property. Horse has many Matches, specifically to the attribute
  // `Match.horseId1`. This attribute can be populated explicitly by a Data Source,
  // directly putting all matching values into memory.
  @OneToMany("horseId1")
  matches: Match[];

  // Represents a SQL view `Horse_default`. All attributes are included by default, but foreign key
  // relationships must be explicitly included in the tree.
  //
  // Determines what values get populated on instantiated methods.
  @DataSource
  readonly default: IncludeTree<Horse> = {
    matches: { horse2: {} },
  };

  // Workers endpoint `domain/Horse/post`, expecting body param `horse`.
  // `D1Database` is injected into the method call.
  //
  // Generates a client method `Horse.post(horse)`
  //
  // By v0.0.3, generic Post methods will be completely generated.
  @POST
  static async post(db: D1Database, horse: Horse): Promise<Result<Horse>> {
    const records = await db
      .prepare("INSERT INTO Horse (id, name, bio) VALUES (?, ?, ?) RETURNING *")
      .bind(horse.id, horse.name, horse.bio)
      .all();

    // `mapSql<Horse>` turns an ORM friendly query result into a list of JSON formatted Horse
    let horseJson = mapSql<Horse>(records.results)[0];
    return { ok: true, status: 200, data: horseJson };
  }

  // Workers endpoint `domain/Horse/get`
  // `D1Database` is injected into the method call.
  //
  // Generates a client method `Horse.get`
  //
  // By v0.0.3, generic get methods will be completely generated.
  @GET
  static async get(db: D1Database, id: number): Promise<Result<Horse>> {
    let records = await db
      .prepare("SELECT * FROM Horse_default WHERE Horse_id = ?")
      .bind(id)
      .run();

    // `mapSql<Horse>` turns an ORM friendly query result into a list of JSON formatted Horse
    let horses = mapSql<Horse>(records.results);

    return { ok: true, status: 200, data: horses[0] };
  }

  // Workers endpoint `domain/Horse/list`
  // `D1Database` is injected into the method call.
  //
  // Generates a client method `Horse.list`
  //
  // By v0.0.3, generic list methods will be completely generated.
  @GET
  static async list(db: D1Database): Promise<Result<Horse[]>> {
    let records = await db.prepare("SELECT * FROM Horse_default").run();

    // `mapSql<Horse>` turns an ORM friendly query result into a list of JSON formatted Horse
    let horses = mapSql<Horse>(records.results);

    return { ok: true, status: 200, data: horses };
  }

  // Workers endpoint `domain/Horse/patch`
  // `D1Database` is injected into the method call.
  //
  // Generates a client method `Horse.patch(horse)`
  //
  // By v0.0.3, generic patch methods will be completely generated.
  @PATCH
  async patch(db: D1Database, horse: Horse): Promise<Result<void>> {
    await db
      .prepare("UPDATE Horse SET name = ?, bio = ? WHERE Horse.id = ?")
      .bind(horse.name, horse.bio, horse.id)
      .run();
    return { ok: true, status: 200 };
  }

  // Workers endpoint `domain/Horse/:id/match`
  // `D1Database` is injected into the method call.
  //
  // Instantiated, so `this` values are populated by the default data source.
  //
  // Generates a client method `horse.match(horse2)`
  @POST
  async match(db: D1Database, horse: Horse): Promise<Result<void>> {
    await db
      .prepare("INSERT INTO Match (horseId1, horseId2) VALUES (?, ?)")
      .bind(this.id, horse.id)
      .run();

    return { ok: true, status: 200 };
  }
}

@D1
class Match {
  // Denotes that `id` is the primary key of table `Match`
  @PrimaryKey
  id: number;

  // A foreign key to the model Horse, denoting that models id.
  @ForeignKey(Horse)
  horseId1: number;

  // Another foreign key to the model Horse, denoting that models id.
  @ForeignKey(Horse)
  horseId2: number;

  // A navigation property. Match has a Horse (because of the FK above),
  // so this attribute can be populated explicitly by a Data Source,
  // directly putting the matching Horse into memory.
  @OneToOne("horseId2")
  horse2: Horse | undefined;
}

export { Match, Horse };
