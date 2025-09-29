//@ts-nocheck
@WranglerEnv
class Env {
  db: D1Database;
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

  @POST
  static async post(
    @Inject { db }: Env,
    horse: Horse
  ): Promise<HttpResult<Horse>> {}

  @GET
  static async get(
    @Inject { db }: Env,
    id: number
  ): Promise<HttpResult<Horse>> {}

  @GET
  static async list(@Inject { db }: Env): Promise<HttpResult<Horse[]>> {}

  @PATCH
  async patch(@Inject { db }: Env, horse: Horse): Promise<HttpResult<void>> {}

  @POST
  async like(@Inject { db }: Env, horse: Horse): Promise<HttpResult<void>> {}

  /*  Random functions for test coverage  */
  @GET
  static async divide(a: number, b: number): Promise<HttpResult<number>> {}
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
