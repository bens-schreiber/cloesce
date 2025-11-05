--- New Models
CREATE TABLE IF NOT EXISTS "CrudHaver" ( "id" integer PRIMARY KEY, "name" text NOT NULL );
CREATE TABLE IF NOT EXISTS "Parent" ( "id" integer PRIMARY KEY, "favoriteChildId" integer, FOREIGN KEY ("favoriteChildId") REFERENCES "Child" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );
CREATE TABLE IF NOT EXISTS "Child" ( "id" integer PRIMARY KEY, "parentId" integer NOT NULL, FOREIGN KEY ("parentId") REFERENCES "Parent" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );

--- New Data Sources
CREATE VIEW IF NOT EXISTS "Parent.withChildren" AS SELECT "Parent"."id" AS "Parent.id", "Parent"."favoriteChildId" AS "Parent.favoriteChildId", "Child"."id" AS "Parent.favoriteChild.id", "Child"."parentId" AS "Parent.favoriteChild.parentId", "Child1"."id" AS "Parent.children.id", "Child1"."parentId" AS "Parent.children.parentId" FROM "Parent" LEFT JOIN "Child" ON "Parent"."favoriteChildId" = "Child"."id" LEFT JOIN "Child" AS "Child1" ON "Parent"."id" = "Child1"."parentId";

--- Cloesce Temporary Table
CREATE TABLE "_cloesce_tmp" ( "path" text PRIMARY KEY, "id" integer NOT NULL );