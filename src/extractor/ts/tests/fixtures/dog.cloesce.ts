@D1
class Dog {
  @PrimaryKey
  id!: number;
  name!: string;
  breed!: number;
  preferred_treat: string | null;

  @GET
  async get_name(db: D1Database, req: Request) {
    const who = new URL(req.url).searchParams.get("name");
    return new Response(JSON.stringify({ hello: who }), {
      headers: { "content-type": "application/json" },
    });
  }

  @GET
  async get_breed(db: D1Database, req: Request) {
    const breed = new URL(req.url).searchParams.get("breed");
    return new Response(JSON.stringify({ hello: breed }), {
      headers: { "content-type": "application/json" },
    });
  }

  @POST
  static async woof(db: D1Database, req: Request, phrase: string) {
    return new Response(JSON.stringify({ phrase }), {
      status: 201,
      headers: { "content-type": "application/json" },
    });
  }
}
