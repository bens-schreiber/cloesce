import {
  D1,
  POST,
  PrimaryKey,
  WranglerEnv,
  PlainOldObject,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };

@PlainOldObject
export class PooA {
  name: string;
  major: string;
}

@PlainOldObject
export class PooB {
  color: string;
}

@PlainOldObject
export class PooC {
  a: PooA;
  b: PooB[];
}

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class PooAcceptYield {
  @PrimaryKey
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
