@D1
class Actions {
  @PrimaryKey id!: number;
  name!: string;

  @POST
  static create(db: D1Database, req: Request, payload: string) {
    return new Response("ok", { status: 201 });
  }

  @PUT
  static update(
    db: D1Database,
    req: Request,
    id: number,
    payload: string | null,
  ) {
    return new Response("ok");
  }

  @PATCH
  static patch(db: D1Database, req: Request, phrase: string) {
    return new Response("ok");
  }

  @DELETE
  static remove(db: D1Database, req: Request, id: number) {
    return new Response("ok");
  }
}
