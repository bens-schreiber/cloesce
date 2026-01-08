import {
  Model,
  PrimaryKey,
  WranglerEnv,
  CRUD,
  POST,
  ForeignKey,
  OneToMany,
  OneToOne,
  DataSource,
  IncludeTree,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

@CRUD(["SAVE", "GET", "LIST"])
@Model
export class CrudHaver {
  @PrimaryKey
  id: Integer;
  name: string;

  @POST
  async notCrud(): Promise<void> { }
}

@CRUD(["SAVE", "GET", "LIST"])
@Model
export class Parent {
  @PrimaryKey
  id: Integer;

  @ForeignKey("Child")
  favoriteChildId: Integer | null;

  @OneToOne("favoriteChildId")
  favoriteChild: Child | undefined;

  @OneToMany("parentId")
  children: Child[];

  @DataSource
  static readonly withChildren: IncludeTree<Parent> = {
    favoriteChild: {},
    children: {},
  };
}

@Model
export class Child {
  @PrimaryKey
  id: Integer;

  @ForeignKey(Parent)
  parentId: Integer;

  @OneToOne("parentId")
  parent: Parent | undefined;
}
