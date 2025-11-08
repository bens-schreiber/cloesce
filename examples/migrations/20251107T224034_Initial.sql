--- New Models
CREATE TABLE IF NOT EXISTS "Horse" (
  "id" integer PRIMARY KEY,
  "name" text NOT NULL,
  "bio" text
);

CREATE TABLE IF NOT EXISTS "Like" (
  "id" integer PRIMARY KEY,
  "horseId1" integer NOT NULL,
  "horseId2" integer NOT NULL,
  FOREIGN KEY ("horseId1") REFERENCES "Horse" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("horseId2") REFERENCES "Horse" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- New Data Sources
CREATE VIEW IF NOT EXISTS "Horse.default" AS
SELECT
  "Horse"."id" AS "id",
  "Horse"."name" AS "name",
  "Horse"."bio" AS "bio",
  "Like"."id" AS "likes.id",
  "Like"."horseId1" AS "likes.horseId1",
  "Like"."horseId2" AS "likes.horseId2",
  "Horse1"."id" AS "likes.horse2.id",
  "Horse1"."name" AS "likes.horse2.name",
  "Horse1"."bio" AS "likes.horse2.bio"
FROM
  "Horse"
  LEFT JOIN "Like" ON "Horse"."id" = "Like"."horseId1"
  LEFT JOIN "Horse" AS "Horse1" ON "Like"."horseId2" = "Horse1"."id";

CREATE VIEW IF NOT EXISTS "Horse.withLikes" AS
SELECT
  "Horse"."id" AS "id",
  "Horse"."name" AS "name",
  "Horse"."bio" AS "bio",
  "Like"."id" AS "likes.id",
  "Like"."horseId1" AS "likes.horseId1",
  "Like"."horseId2" AS "likes.horseId2"
FROM
  "Horse"
  LEFT JOIN "Like" ON "Horse"."id" = "Like"."horseId1";

--- Cloesce Temporary Table
CREATE TABLE "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);