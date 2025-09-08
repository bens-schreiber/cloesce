import { D1Database } from "@cloudflare/workers-types";
import { D1, POST, PrimaryKey } from "cloesce";

/// A Cloesce Model, representing: D1, Workers and the Client API.
///
/// Instantiated methods act on a hydrated instance of a D1 table row,
/// static methods are apart of the same namespace but do not pertain to an instance.
///
/// Methods can take in parameters which are serialized to the request body only (GET does not work in v0.0.1)
/// `D1Database` can be dependency injected into a method.
@D1
export class Person {
  // Explicit primary key hint for the extractor
  @PrimaryKey
  id!: number;
  name!: string;

  // Denoting null cannot be done with ? as that also means undefined
  ssn!: string | null;

  /// Replies with the phrase: "<name> <social security number> <favorite_number>"
  @POST
  async speak(favorite_number: number) {
    let res = `${this.name} ${this.ssn} ${favorite_number}`;

    // v0.0.1 only supports JSON return types
    return new Response(JSON.stringify(res));
  }

  /// A basic 'POST Person' endpoint, returning a newly inserted Person in JSON
  @POST
  static async post(db: D1Database, name: string, ssn: string | null) {
    // db is dependency injected from the Cloudflare environment
    let result = await db
      .prepare("INSERT INTO Person (name, ssn) VALUES (?, ?) RETURNING *")
      .bind(name, ssn)
      .run();

    return new Response(JSON.stringify({ ...result.results[0] }), {
      headers: { "content-type": "application/json" },
    });
  }
}
