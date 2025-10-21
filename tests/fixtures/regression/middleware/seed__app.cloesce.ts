import { CloesceApp, WranglerEnv, D1, PrimaryKey, CRUD } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
@CRUD(["POST"])
export class Model {
  @PrimaryKey
  id: number;
}

const app: CloesceApp = new CloesceApp();

app.use((request: Request, env, ir) => {
  if (request.method === "POST") {
    return { ok: false, status: 401, message: "POST methods aren't allowed." };
  }
});

export default app;
