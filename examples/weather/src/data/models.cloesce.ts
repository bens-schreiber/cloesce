import { Get, Post, HttpResult, Integer, Model, R2, Inject, Crud } from "cloesce/backend";
import { R2ObjectBody, ReadableStream } from "@cloudflare/workers-types";
import { Env } from "./main.cloesce";

@Model("db")
export class Weather {
    // Cloesce interprets this is a primary key.
    // Optionally, decorate with @PrimaryKey
    id: Integer;

    // Foreign key to WeatherReport
    // Optionally, decorate with @ForeignKey<WeatherReport>(x => x.weatherReportId)
    weatherReportId: Integer;

    // Navigation property to weatherReportId
    weatherReport: WeatherReport | undefined;

    dateTime: Date;
    location: string;
    temperature: number;
    condition: string;

    @R2("weather/photo/{id}", "bucket")
    photo: R2ObjectBody | undefined;

    @Post()
    async uploadPhoto(@Inject env: Env, stream: ReadableStream): Promise<HttpResult<void>> {
        await env.bucket.put(`weather/photo/${this.id}`, stream);
        return HttpResult.ok(200);
    }

    @Get({ includeTree: { photo: {} } })
    downloadPhoto(): HttpResult<ReadableStream> {
        if (!this.photo) {
            return HttpResult.fail(404, "Photo not found");
        }
        return HttpResult.ok(200, this.photo.body);
    }
}

@Crud("GET", "LIST", "SAVE")
@Model("db")
export class WeatherReport {
    // Cloesce assumes this is a primary key.
    // Optionally, decorate with @PrimaryKey
    id: Integer;

    title: string;
    description: string;

    // Cloesce infers this relation via naming conventions
    weatherEntries: Weather[];
}
