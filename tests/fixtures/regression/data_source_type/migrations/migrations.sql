-- Models
CREATE TABLE "Foo" ( "id" integer PRIMARY KEY );
CREATE TABLE "NoDs" ( "id" integer PRIMARY KEY );
CREATE TABLE "OneDs" ( "id" integer PRIMARY KEY );

-- Views / Data Sources
CREATE VIEW "Foo.baz" AS SELECT "Foo"."id" AS "Foo.id" FROM "Foo";
CREATE VIEW "OneDs.default" AS SELECT "OneDs"."id" AS "OneDs.id" FROM "OneDs";

-- Cloesce
CREATE TABLE "_cloesce_tmp" ( "path" text PRIMARY KEY, "id" integer NOT NULL );