-- Models
CREATE TABLE "NullabilityChecks" ( "id" integer PRIMARY KEY, "notNullableString" text NOT NULL, "nullableString" text );

-- Views / Data Sources


-- Cloesce
CREATE TABLE "_cloesce_tmp" ( "path" text PRIMARY KEY, "id" integer NOT NULL );