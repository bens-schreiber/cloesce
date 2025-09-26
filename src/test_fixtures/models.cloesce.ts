//@ts-nocheck
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
  static async post(db: D1Database, horse: Horse): Promise<HttpResult<Horse>> {
    const records = await db
      .prepare("INSERT INTO Horse (id, name, bio) VALUES (?, ?, ?) RETURNING *")
      .bind(horse.id, horse.name, horse.bio)
      .all();

    let horseRes = modelsFromSql(Horse, records.results, Horse.default)[0];

    return { ok: true, status: 200, data: horseRes };
  }

  @GET
  static async get(db: D1Database, id: number): Promise<HttpResult<Horse>> {
    let records = await db
      .prepare("SELECT * FROM Horse_default WHERE Horse_id = ?")
      .bind(id)
      .run();
    let horses = modelsFromSql(Horse, records.results, Horse.default);
    return { ok: true, status: 200, data: horses[0] };
  }

  @GET
  static async list(db: D1Database): Promise<HttpResult<Horse[]>> {
    let records = await db.prepare("SELECT * FROM Horse_default").run();
    let horses = modelsFromSql(Horse, records.results, Horse.default);
    return { ok: true, status: 200, data: horses };
  }

  @PATCH
  async patch(db: D1Database, horse: Horse): Promise<HttpResult<void>> {
    await db
      .prepare("UPDATE Horse SET name = ?, bio = ? WHERE Horse.id = ?")
      .bind(horse.name, horse.bio, horse.id)
      .run();
    return { ok: true, status: 200 };
  }

  @POST
  async like(db: D1Database, horse: Horse): Promise<HttpResult<void>> {
    await db
      .prepare("INSERT INTO Like (horseId1, horseId2) VALUES (?, ?)")
      .bind(this.id, horse.id)
      .run();
    return { ok: true, status: 200 };
  }

  /*  Random functions for test coverage  */
  @GET
  static async divide(a: number, b: number): Promise<HttpResult<number>> {
    if (b != 0) {
      return { ok: true, status: 200, data: a / b };
    } else {
      return { ok: false, status: 400, message: "divided by 0" };
    }
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
