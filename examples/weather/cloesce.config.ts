import { CloesceConfigOptions } from "cloesce";

const config: CloesceConfigOptions = {
    srcPaths: ["./src/schema"],
    workersUrl: "http://localhost:5000/api",
    wranglerConfigFormat: "jsonc",
};

export default config;