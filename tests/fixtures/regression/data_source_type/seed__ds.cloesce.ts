import {
  Model,
  POST,
  WranglerEnv,
  DataSourceOf,
  IncludeTree,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model()
export class NoDs {
  id: Integer;
}

@Model()
export class OneDs {
  id: Integer;

  static readonly default: IncludeTree<OneDs> = {};
}

@Model()
export class Foo {
  id: Integer;

  static readonly baz: IncludeTree<Foo> = {};

  @POST
  bar(
    customDs: DataSourceOf<Foo>,
    oneDs: DataSourceOf<OneDs>,
    noDs: DataSourceOf<NoDs>,
    poo: Poo,
  ) { }
}

export class Poo {
  ds: DataSourceOf<Foo>;
}
