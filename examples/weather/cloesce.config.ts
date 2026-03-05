import { defineConfig } from "cloesce/config";

const config = defineConfig({
    srcPaths: ["./src/data"],
    workersUrl: "http://localhost:5000/api",
    migrationsPath: "./migrations",
});


export default config;
