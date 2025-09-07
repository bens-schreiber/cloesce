import { D1Database } from "@cloudflare/workers-types";
import { D1, GET, POST, PrimaryKey } from "cloesce";

@D1
export class Person {
  @PrimaryKey
  id!: number;
  name!: string;
  ssn!: string | null;

  @POST
  async speak(favorite_number: number) {
    let res = `${this.name} ${this.ssn} ${favorite_number}`;
    return new Response(JSON.stringify(res));
  }

  @POST
  static async post(db: D1Database, name: string, ssn: string | null) {
    let result = await db
      .prepare("INSERT INTO Person (name, ssn) VALUES (?, ?) RETURNING *")
      .bind(name, ssn)
      .run();

    return new Response(JSON.stringify({ ...result.results[0] }), {
      headers: { "content-type": "application/json" },
    });
  }
}
