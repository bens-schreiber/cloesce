import { cloesce, Env } from "./backend.js";
import { PaginatedKVModel } from "./backend.js";

const PaginatedKvImpl = PaginatedKVModel.impl({
    acceptPaginated(ps) {
        return ps;
    },

});

export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(PaginatedKvImpl);
        return await app.run(request, env);
    }
}