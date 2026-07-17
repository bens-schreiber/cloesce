--- New Models
CREATE TABLE IF NOT EXISTS "SubReddit" (
  "id" integer PRIMARY KEY,
  "title" text NOT NULL,
  "description" text NOT NULL,
  "lastPostId" integer NOT NULL
);

CREATE TABLE IF NOT EXISTS "SubRedditPost" (
  "postId" integer PRIMARY KEY,
  "subRedditId" integer NOT NULL,
  FOREIGN KEY ("subRedditId") REFERENCES "SubReddit" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "$cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);