import { describe, test, expect } from "vitest";
import { Weather, WeatherReport } from "@api/main.js";
import { upgraded } from "./setup.js";

// This does not use any client stubs; it interacts directly with the bound D1/R2 instances.
describe("Cloudflare Workers Integration Tests", () => {
  test("Download a thumbnail", async () => {
    // Arrange
    const env = upgraded();
    const testData = "test-data";

    const report = (
      await WeatherReport.Orm.save(env, {
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
    ).value!;

    await Weather.uploadPhoto(report.weatherEntries[0], env, testData as any);

    // Act
    const weatherEntries = (await Weather.Default.list(env, 0, 100)).data!;
    const photo = Weather.downloadPhoto(weatherEntries[0]);

    // Assert
    expect(photo.ok).toBe(true);
    const downloadedText = await new Response(photo.data as any).text();
    expect(downloadedText).toBe(testData);
  });
});
