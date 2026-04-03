import { CloesceConfigOptions } from "cloesce";

const config: CloesceConfigOptions = {
    srcPaths: ["./src/data"],
    workersUrl: "http://localhost:5000/api",
    migrationsPath: "./customDir",
    wranglerConfigFormat: "jsonc",
};

export default config;
