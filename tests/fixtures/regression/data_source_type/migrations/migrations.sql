-- Models
CREATE TABLE "Foo" ( "id" integer PRIMARY KEY );

-- Views / Data Sources
CREATE VIEW "Foo.baz" AS SELECT "Foo"."id" AS "Foo.id" FROM "Foo";

-- Cloesce
CREATE TABLE "_cloesce_tmp" ( "path" text PRIMARY KEY, "id" integer NOT NULL );