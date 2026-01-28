import { Model, POST, WranglerEnv, Integer } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

export class PooA {
  name: string;
  major: string;
}

export class PooB {
  color: string;
}

export class PooC {
  a: PooA;
  b: PooB[];
}

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model()
export class PooAcceptYield {
  id: Integer;

  @POST
  static acceptPoos(a: PooA, b: PooB, c: PooC) {}

  @POST
  static yieldPoo(): PooC {
    return {
      a: {
        name: "name",
        major: "major",
      },
      b: [
        {
          color: "color",
        },
      ],
    };
  }
}
