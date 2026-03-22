import { D1Database } from "@cloudflare/workers-types";
import { Crud, Integer, Model, WranglerEnv } from "cloesce/backend";

@WranglerEnv
export class Env {
    db1: D1Database;
    db2: D1Database;
}

@Crud("GET", "SAVE", "LIST")
@Model("db1")
export class DB1Model {
    id: Integer;
    someColumn: string;
}

@Crud("GET", "SAVE", "LIST")
@Model("db2")
export class DB2Model {
    id: Integer;
    someColumn: string;
}
