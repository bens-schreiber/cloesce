import {
  D1,
  POST,
  PrimaryKey,
  WranglerEnv,
  DataSourceOf,
  DataSource,
  IncludeTree,
} from "cloesce/backend";

import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  static readonly baz: IncludeTree<Foo> = {};

  @POST
  bar(customDs: DataSourceOf<Foo>) {}
}
