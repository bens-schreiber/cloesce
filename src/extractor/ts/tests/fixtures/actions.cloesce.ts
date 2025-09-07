import { D1, D1Db, POST, PUT, PATCH, DELETE, PrimaryKey } from "cloesce-ts";

@D1
class Actions {
  @PrimaryKey id!: number;
  name!: string;

  @POST
  static create(db: D1Db, req: Request, payload: string) {
    return new Response("ok", { status: 201 });
  }

  @PUT
  static update(db: D1Db, req: Request, id: number, payload: string | null) {
    return new Response("ok");
  }

  @PATCH
  static patchlol(db: D1Db, req: Request, phrase: string) {
    return new Response("ok");
  }

  @DELETE
  static remove(db: D1Db, req: Request, id: number) {
    return new Response("ok");
  }
}
