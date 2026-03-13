import { defineConfig } from "cloesce/config";
import { Weather } from "./src/data/models.cloesce";

const config = defineConfig({
    srcPaths: ["./src/data"],
    workersUrl: "http://localhost:5000/api",
    migrationsPath: "./customDir",
});

config.model(Weather, builder => {
    builder.unique("dateTime", "location");
});

export default config;
