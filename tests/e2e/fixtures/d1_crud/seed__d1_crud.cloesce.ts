import { Model, WranglerEnv, Post, Integer, DataSource } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model(["SAVE", "GET", "LIST"])
export class CrudHaver {
  id: Integer;
  name: string;

  @Post()
  async notCrud(): Promise<void> { }
}

@Model(["SAVE", "GET", "LIST"])
export class Parent {
  id: Integer;

  favoriteChildId: Integer | null;
  favoriteChild: Child | undefined;

  children: Child[];

  static readonly withChildren: DataSource<Parent> = {
    includeTree: {
      favoriteChild: {},
      children: {},
    },
  };
}

@Model()
export class Child {
  id: Integer;
  parentId: Integer;
  parent: Parent | undefined;
}
