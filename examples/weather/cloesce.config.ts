import { CloesceConfig } from "cloesce";

const config: CloesceConfig = {
    srcPaths: ["./src/data"],
    workersUrl: "http://localhost:5000/api",
    migrationsPath: "./customDir",
    wranglerConfigFormat: "jsonc",
};

export default config;
