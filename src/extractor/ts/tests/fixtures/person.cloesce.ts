import { D1, D1Db, GET, POST, PrimaryKey } from "cloesce-ts";

@D1
class Person {
  @PrimaryKey
  id!: number;
  name!: string;
  middle_name: string | null;

  @GET
  async foo(db: D1Db, req: Request) {
    const who = new URL(req.url).searchParams.get("name") ?? "world";
    return new Response(JSON.stringify({ hello: who }), {
      headers: { "content-type": "application/json" },
    });
  }

  @POST
  static async speak(db: D1Db, req: Request, phrase: string) {
    return new Response(JSON.stringify({ phrase }), {
      status: 201,
      headers: { "content-type": "application/json" },
    });
  }
}
