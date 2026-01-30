import { GET, POST, HttpResult, IncludeTree, Integer, Model, R2, Inject } from "cloesce/backend";
import { R2ObjectBody, ReadableStream } from "@cloudflare/workers-types";
import { Env } from "./main.cloesce";

@Model()
export class Weather {
    // Cloesce interprets this is a primary key.
    // Optionally, decorate with @PrimaryKey
    id: Integer;

    // Foreign key to WeatherReport
    // Optionally, decorate with @ForeignKey
    weatherReportId: Integer;

    // Navigation property to weatherReportId
    // Optionally, decorate with @OneToOne<Weather>(w => w.weatherReportId)
    weatherReport: WeatherReport | undefined;

    dateTime: Date;
    location: string;
    temperature: number;
    condition: string;

    @R2("weather/photo/{id}", "bucket")
    photo: R2ObjectBody | undefined;

    // Hydrates the photo when the client calls "withPhoto"
    static readonly withPhoto: IncludeTree<Weather> = {
        photo: {}
    }

    @POST
    async uploadPhoto(@Inject env: Env, stream: ReadableStream): Promise<HttpResult<void>> {
        await env.bucket.put(`weather/photo/${this.id}`, stream);
        return HttpResult.ok(200);
    }

    @GET
    downloadPhoto(): HttpResult<ReadableStream> {
        if (!this.photo) {
            return HttpResult.fail(404, "Photo not found");
        }
        return HttpResult.ok(200, this.photo.body);
    }
}

@Model(["GET", "LIST", "SAVE"])
export class WeatherReport {
    // Cloesce assumes this is a primary key.
    // Optionally, decorate with @PrimaryKey
    id: Integer;

    title: string;
    description: string;

    // Cloesce assumes this is a foreign key to Weather.weatherReportId
    // Optionally, or if multiple FKs exist, decorate with
    // @OneToMany<Weather>(w => w.weatherReportId)
    weatherEntries: Weather[];

    // Hydrates the weatherEntries when the client calls "withWeatherEntries"
    static readonly withWeatherEntries: IncludeTree<WeatherReport> = {
        weatherEntries: {}
    }
}
