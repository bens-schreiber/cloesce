-- Models
CREATE TABLE "CrudHaver" ( "id" integer PRIMARY KEY, "name" text NOT NULL );
CREATE TABLE "Parent" ( "id" integer PRIMARY KEY, "favoriteChildId" integer, FOREIGN KEY ("favoriteChildId") REFERENCES "Child" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );
CREATE TABLE "Child" ( "id" integer PRIMARY KEY, "parentId" integer NOT NULL, FOREIGN KEY ("parentId") REFERENCES "Parent" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );

-- Views / Data Sources
CREATE VIEW "Parent.withChildren" AS SELECT "Parent"."id" AS "Parent.id", "Parent"."favoriteChildId" AS "Parent.favoriteChildId", "Child"."id" AS "Parent.children.id", "Child"."parentId" AS "Parent.children.parentId", "Child1"."id" AS "Parent.favoriteChild.id", "Child1"."parentId" AS "Parent.favoriteChild.parentId" FROM "Parent" LEFT JOIN "Child" ON "Parent"."id" = "Child"."parentId" LEFT JOIN "Child" AS "Child1" ON "Parent"."favoriteChildId" = "Child1"."id";

-- Cloesce
CREATE TABLE "_cloesce_tmp" ( "path" text PRIMARY KEY, "id" integer NOT NULL );