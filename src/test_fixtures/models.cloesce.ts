// @ts-nocheck

@D1
class Horse {
  @PrimaryKey
  id: number;

  name: string;
  bio: string | null;

  @OneToMany("horseId1")
  matches: Match[];

  @DataSource
  readonly default: IncludeTree<Horse> = {
    matches: { horse2: {} },
  };

  @POST
  static async post(db: D1Database, horse: Horse): Promise<Result<Horse>> {
    let records = await db
      .prepare(
        "INSERT INTO Horse (id, name, bio) VALUES (:id, :name, :bio) RETURNING *",
      )
      .bind(horse)
      .run();

    let horseJson = mapSql<Horse>(records.results)[0];
    return Result.ok(horseJson);
  }

  @GET
  static async get(db: D1Database, id: number): Promise<Result<Horse[]>> {
    let records = await db
      .prepare("SELECT * FROM Horse_default WHERE id = ?")
      .bind(id)
      .run();

    let horses = mapSql<Horse>(records.results);
    return Result.ok(horses);
  }

  @GET
  static async list(db: D1Database): Promise<Result<Horse[]>> {
    let records = await db.prepare("SELECT * FROM Horse_default").run();

    let horses = mapSql<Horse>(records.results);
    return Result.ok(horses);
  }

  @PATCH
  async patch(db: D1Database, horse: Horse): Promise<Result> {
    await db
      .prepare("UPDATE Horse SET name = :name, bio = :bio WHERE Horse.id = :id")
      .bind(horse)
      .run();
    return Result.ok();
  }

  @POST
  async match(db: D1Database, horse: Horse): Promise<Result> {
    await db
      .prepare("INSERT INTO Match (horseId1, horseId2) VALUES (?, ?)")
      .bind(this.id, horse.id)
      .run();

    return Result.ok();
  }
}

@D1
class Match {
  @PrimaryKey
  id: number;

  @ForeignKey(Horse)
  horseId1: number;

  @ForeignKey(Horse)
  horseId2: number;

  @OneToOne("horseId2")
  horse2: Horse | undefined;
}

export { Match, Horse };
