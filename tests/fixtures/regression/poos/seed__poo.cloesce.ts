import {
  D1,
  POST,
  PrimaryKey,
  WranglerEnv,
  Inject,
  PlainOldObject,
} from "cloesce/backend";
type D1Database = {};

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
  b: PooB;
}

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class PooAcceptYield {
  @PrimaryKey
  id: number;

  @POST
  acceptPoos(a: PooA, b: PooB, c: PooC) {}

  @POST
  yieldPoo(): PooC {
    return {
      a: {
        name: "",
        major: "",
      },
      b: {
        color: "",
      },
    };
  }
}
