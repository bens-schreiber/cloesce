import { describe, test, expect } from "vitest";
import { app } from "./setup.js";

// This test does not use any client stubs, but instead directly calls
// backend methods.
describe("Cloudflare Workers Integration Tests", () => {
  test("Download a thumbnail", async () => {
    // Arrange
    const env = app.env;
    const weather = env.db.weather;
    const testData = "test-data";

    const report = (
      await env.db.weatherReport.save({
        title: "Test Report",
        description: "This is a test weather report.",
        weatherEntries: [
          {
            dateTime: new Date(),
            location: "Test Location",
            temperature: 25,
            condition: "Sunny",
          },
        ],
      })
    ).data!;

    await weather.uploadPhoto(report.weatherEntries[0], testData as any);

    // Act
    const weatherEntries = (await weather.list(0, 100)).data!;
    const photo = await weather.downloadPhoto(weatherEntries[0]);

    // Assert
    expect(photo.ok).toBe(true);
    const downloadedText = await new Response(photo.data as any).text();
    expect(downloadedText).toBe(testData);
  });
});
