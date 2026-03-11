import { KV, KValue, Paginated, WranglerEnv, KeyParam, Model, DataSource, Integer, Post, Crud } from "cloesce/backend";
import { D1Database, KVNamespace } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
  namespace: KVNamespace;
  otherNamespace: KVNamespace;
}

@Crud("GET")
@Model()
export class PureKVModel {
  @KeyParam
  id: string;

  @KV("path/to/data/{id}", "namespace")
  data: KValue<unknown>;

  @KV("path/to/other/{id}", "otherNamespace")
  otherData: KValue<string>;
}

@Crud("GET", "SAVE", "LIST")
@Model("db")
export class D1BackedModel {
  id: Integer;

  @KeyParam
  keyParam: string;

  someColumn: number;
  someOtherColumn: string;

  @KV("d1Backed/{id}/{keyParam}/{someColumn}/{someOtherColumn}", "namespace")
  kvData: KValue<unknown>;
}

@Crud("GET")
@Model()
export class PaginatedKVModel {
  @KeyParam
  id: string;

  @KV("paginated/items/", "namespace")
  items: Paginated<KValue<unknown>>;

  @Post()
  static acceptPaginated(ps: Paginated<KValue<unknown>>): Paginated<KValue<unknown>> {
    return ps;
  }
}
