import {
  D1,
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

@WranglerEnv
export class Env {
  db: D1Database;
}

@CRUD(["POST", "GET", "LIST"])
@D1
export class CrudHaver {
  @PrimaryKey
  id: number;
  name: string;

  @POST
  async notCrud(): Promise<void> {}
}

@CRUD(["POST", "GET", "LIST"])
@D1
export class Parent {
  @PrimaryKey
  id: number;

  @ForeignKey("Child")
  favoriteChildId: number | null;

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

@D1
export class Child {
  @PrimaryKey
  id: number;

  @ForeignKey(Parent)
  parentId: number;

  @OneToOne("parentId")
  parent: Parent | undefined;
}
