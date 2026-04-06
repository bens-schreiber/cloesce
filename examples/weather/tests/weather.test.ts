import { Miniflare } from "miniflare";
import { describe, test, expect, beforeAll } from "vitest";
import * as Cloesce from "@cloesce/backend.js";
import { Weather } from "@api/main.js"
import { cloesce } from "@cloesce/backend.js";

async function createTestEnv() {
    const mf = new Miniflare({
        modules: true,
        script: `export default { async fetch() { return new Response("Hello!"); } }`,
        d1Databases: ["db"],
        r2Buckets: ["bucket"],
    });

    const db = await mf.getD1Database("db");
    const bucket = await mf.getR2Bucket("bucket");
    const env = { db, bucket } as any;

    // Run any necessary migrations
    // TODO: Does Cloudflare have a way to do this automatically in tests?
    await db.prepare(`
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
  "temperature" integer NOT NULL,
  "condition" text NOT NULL,
  FOREIGN KEY ("weatherReportId") REFERENCES "WeatherReport" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);
  `).run();

    return env;
}

beforeAll(() => cloesce());

// Here we will test our Cloesce models against a Miniflare environment.
// This does not use any client stubs; it interacts directly with the Miniflare instance.
describe("Miniflare Integration Tests", () => {
    test("Download a thumbnail", async () => {
        // Arrange
        const env = await createTestEnv();
        const testData = "test-data";

        const report = (await Cloesce.WeatherReport.save(env, {
            title: "Test Report",
            description: "This is a test weather report.",
            weatherEntries: [{
                dateTime: new Date(),
                location: "Test Location",
                temperature: 25,
                condition: "Sunny"
            }]
        }))!;

        await new Weather().uploadPhoto(report.weatherEntries[0], env, testData as any);

        // Act
        const weatherEntries = (await Cloesce.Weather.DataSources.Default.list(env, 0, 100))!;
        const photo = new Weather().downloadPhoto(weatherEntries[0]);

        // Assert
        expect(photo.ok).toBe(true);
        const downloadedText = await new Response(photo.data as any).text();
        expect(downloadedText).toBe(testData);
    });
});