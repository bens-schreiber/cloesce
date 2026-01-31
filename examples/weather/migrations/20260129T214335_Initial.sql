--- New Models
CREATE TABLE IF NOT EXISTS "WeatherReport" (
  "id" integer PRIMARY KEY,
  "title" text NOT NULL,
  "description" text NOT NULL
);

CREATE TABLE IF NOT EXISTS "Weather" (
  "id" integer PRIMARY KEY,
  "weatherReportId" integer NOT NULL,
  "dateTime" text NOT NULL,
  "location" text NOT NULL,
  "temperature" real NOT NULL,
  "condition" text NOT NULL,
  FOREIGN KEY ("weatherReportId") REFERENCES "WeatherReport" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);