import {
  Model,
  GET,
  POST,
  Inject,
  OneToMany,
  OneToOne,
  ForeignKey,
  IncludeTree,
  Orm,
  Integer,
} from "cloesce/backend";
import { Env } from "./main.cloesce";

@Model(["GET", "LIST", "SAVE"])
export class Horse {
  id: Integer;
  name: string;
  bio: string | null;

  @OneToMany<Like>(l => l.horse1Id)
  likes: Like[];

  static readonly default: IncludeTree<Horse> = {
    likes: { horse2: {} },
  };

  static readonly withLikes: IncludeTree<Horse> = {
    likes: {},
  };

  @POST
  async like(@Inject env: Env, horse: Horse) {
    const orm = Orm.fromEnv(env);
    await orm.upsert(Like, {
      horse1Id: this.id,
      horse2Id: horse.id,
    });
  }

  @GET
  async matches(@Inject env: Env): Promise<Horse[]> {
    const selectHorse = Orm.select(Horse, {
      includeTree: Horse.default
    });

    const sql = `
    WITH PersonView as (${selectHorse})
    SELECT * FROM PersonView
    WHERE [likes.horse2.id] = ?
    AND [id] IN (
      SELECT [likes.horse2.id]
      FROM PersonView
      WHERE [id] = ?
    )`;

    const records = await env.db.prepare(sql).bind(this.id, this.id).all();
    return Orm.map(Horse, records, Horse.default);
  }
}

@Model()
export class Like {
  id: Integer;

  @ForeignKey(Horse)
  horse1Id: Integer;

  horse2Id: Integer;
  horse2: Horse | undefined;
}
