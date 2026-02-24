import {
  Model,
  Post,
  WranglerEnv,
  DataSource,
  Integer,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

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

  static readonly default: DataSource<OneDs> = {};
}

@Model()
export class Foo {
  id: Integer;

  static readonly baz: DataSource<Foo> = {};

  @Post(Foo.baz)
  bar(
    customDs: DataSource<Foo>,
    oneDs: DataSource<OneDs>,
    noDs: DataSource<NoDs>,
    poo: Poo,
  ) { }
}

export class Poo {
  ds: DataSource<Foo>;
}
