import { cloesce, Env } from "./backend.js";

export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        return await app.run(request, env);
    }
}