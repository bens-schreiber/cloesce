import { Miniflare } from "miniflare";
import { describe, test, expect, beforeAll } from "vitest";
import { Orm, CloesceApp } from "cloesce/backend";
import { cidl, constructorRegistry } from "@generated/workers";
import { Weather, WeatherReport } from "@data/models.cloesce";

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
    CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);
  `).run();

    return { env, orm: Orm.fromEnv(env) };
}

// Cloesce must be initialized before utilizing any ORM features.
// It takes in the generated Cloesce Interface Definition Language (CIDL)
// and the generated constructor registry. Both may be imported from
// "@generated/workers" as shown above.
beforeAll(() => CloesceApp.init(cidl as any, constructorRegistry));

// Here we will test our Cloesce models against a Miniflare environment.
// This does not use any client stubs; it interacts directly with the Miniflare instance.
describe("Miniflare Integration Tests", () => {
    test("Download a thumbnail", async () => {
        // Arrange
        const { env, orm } = await createTestEnv();
        const testData = "test-data";

        const report = await orm.upsert(
            WeatherReport,
            {
                title: "Test Report",
                description: "This is a test weather report.",
                weatherEntries: [{
                    dateTime: new Date(),
                    location: "Test Location",
                    temperature: 25,
                    condition: "Sunny"
                }]
            },
            WeatherReport.withWeatherEntries
        );

        await report!.weatherEntries[0].uploadPhoto(env, testData as any);

        // Act
        const weatherEntries = await orm.list(Weather, Weather.withPhoto);
        const photo = weatherEntries[0].downloadPhoto();

        // Assert
        expect(photo.ok).toBe(true);
        const downloadedText = await new Response(photo.data as any).text();
        expect(downloadedText).toBe(testData);
    });
});