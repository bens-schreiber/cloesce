import { KV, KValue, WranglerEnv, KeyParam, Model, DataSource, Integer } from "cloesce/backend";
import { D1Database, KVNamespace } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
  namespace: KVNamespace;
  otherNamespace: KVNamespace;
}

@Model(["GET", "SAVE"])
export class PureKVModel {
  @KeyParam
  id: string;

  @KV("path/to/data/{id}", "namespace")
  data: KValue<unknown>;

  @KV("path/to/other/{id}", "otherNamespace")
  otherData: KValue<string>;
}

@Model(["GET", "SAVE", "LIST"])
export class D1BackedModel {
  id: Integer;

  @KeyParam
  keyParam: string;

  someColumn: number;
  someOtherColumn: string;

  @KV("d1Backed/{id}/{keyParam}/{someColumn}/{someOtherColumn}", "namespace")
  kvData: KValue<unknown>;
}
