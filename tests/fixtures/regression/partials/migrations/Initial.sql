--- New Models
CREATE TABLE IF NOT EXISTS "Dog" ( "id" integer PRIMARY KEY, "name" text NOT NULL, "age" integer NOT NULL );


--- Cloesce Temporary Table
CREATE TABLE "_cloesce_tmp" ( "path" text PRIMARY KEY, "id" integer NOT NULL );