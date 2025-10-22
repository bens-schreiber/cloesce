import {
  D1,
  POST,
  PrimaryKey,
  WranglerEnv,
  DataSourceOf,
  DataSource,
  IncludeTree,
  PlainOldObject,
} from "cloesce/backend";

import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class NoDs {
  @PrimaryKey
  id: number;
}

@D1
export class OneDs {
  @PrimaryKey
  id: number;

  @DataSource
  static readonly default: IncludeTree<OneDs> = {};
}

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  static readonly baz: IncludeTree<Foo> = {};

  @POST
  bar(
    customDs: DataSourceOf<Foo>,
    oneDs: DataSourceOf<OneDs>,
    noDs: DataSourceOf<NoDs>
  ) {}
}

@PlainOldObject
export class Poo {
  ds: DataSourceOf<Foo>;
}
