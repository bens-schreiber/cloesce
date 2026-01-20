import {
  Model,
  POST,
  PrimaryKey,
  WranglerEnv,
  DataSourceOf,
  DataSource,
  IncludeTree,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model
export class NoDs {
  @PrimaryKey
  id: Integer;
}

@Model
export class OneDs {
  @PrimaryKey
  id: Integer;

  static readonly default: IncludeTree<OneDs> = {};
}

@Model
export class Foo {
  @PrimaryKey
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
