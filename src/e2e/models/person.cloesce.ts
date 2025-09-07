import { D1Database } from "@cloudflare/workers-types";
import { D1, GET, POST, PrimaryKey } from "cloesce";

@D1
export class Person {
  @PrimaryKey
  id!: number;
  name!: string;
  ssn!: string | null;

  @GET
  async speak(req: Request, count: number) {
    let res = `hello I am ${this.name} and my ssn is ${this.ssn}!\n`.repeat(
      count
    );
    return new Response(JSON.stringify({ res, "my request deets": req }));
  }

  @POST
  static async post(db: D1Database, name: string, ssn: string | null) {
    let result = await db
      .prepare("INSERT INTO users (name, ssn) VALUES (?, ?)")
      .bind(name, ssn)
      .run();
    let id = result.lastInsertRowId;

    return new Response(JSON.stringify({ id, name, ssn }), {
      headers: { "content-type": "application/json" },
    });
  }
}
