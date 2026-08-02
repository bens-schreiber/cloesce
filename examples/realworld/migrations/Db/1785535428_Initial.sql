--- New Models
CREATE TABLE IF NOT EXISTS "Tag" (
  "id" integer PRIMARY KEY,
  "name" text UNIQUE NOT NULL
);

CREATE TABLE IF NOT EXISTS "User" (
  "id" integer PRIMARY KEY,
  "username" text UNIQUE NOT NULL,
  "email" text UNIQUE NOT NULL,
  "bio" text NOT NULL,
  "image" text,
  "passwordHash" text NOT NULL
);

CREATE TABLE IF NOT EXISTS "Article" (
  "id" integer PRIMARY KEY,
  "authorId" integer NOT NULL,
  "slug" text UNIQUE NOT NULL,
  "title" text NOT NULL,
  "description" text NOT NULL,
  "body" text NOT NULL,
  "createdAt" text NOT NULL,
  "updatedAt" text NOT NULL,
  FOREIGN KEY ("authorId") REFERENCES "User" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "Follow" (
  "followerId" integer NOT NULL,
  "followeeId" integer NOT NULL,
  PRIMARY KEY ("followerId", "followeeId"),
  FOREIGN KEY ("followeeId") REFERENCES "User" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("followerId") REFERENCES "User" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "ArticleTag" (
  "articleId" integer NOT NULL,
  "tagId" integer NOT NULL,
  PRIMARY KEY ("articleId", "tagId"),
  FOREIGN KEY ("articleId") REFERENCES "Article" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("tagId") REFERENCES "Tag" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "Comment" (
  "id" integer PRIMARY KEY,
  "articleId" integer NOT NULL,
  "authorId" integer NOT NULL,
  "body" text NOT NULL,
  "createdAt" text NOT NULL,
  "updatedAt" text NOT NULL,
  FOREIGN KEY ("articleId") REFERENCES "Article" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("authorId") REFERENCES "User" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "Favorite" (
  "userId" integer NOT NULL,
  "articleId" integer NOT NULL,
  PRIMARY KEY ("userId", "articleId"),
  FOREIGN KEY ("articleId") REFERENCES "Article" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("userId") REFERENCES "User" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "$cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);