--- New Models
CREATE TABLE IF NOT EXISTS "Foo" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "NoDs" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "OneDs" ("id" integer PRIMARY KEY);

--- New Data Sources
CREATE VIEW IF NOT EXISTS "Foo.baz" AS
SELECT
  "Foo"."id" AS "id"
FROM
  "Foo";

CREATE VIEW IF NOT EXISTS "OneDs.default" AS
SELECT
  "OneDs"."id" AS "id"
FROM
  "OneDs";

--- Cloesce Temporary Table
CREATE TABLE "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);