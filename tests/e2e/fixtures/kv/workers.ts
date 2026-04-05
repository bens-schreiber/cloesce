import { Paginated, KValue, HttpResult } from "cloesce";
import { cloesce, Env } from "./backend.js";
import { PaginatedKVModel } from "./backend.js";

class PaginatedKvImpl extends PaginatedKVModel.Api {
    acceptPaginated(ps: Paginated<KValue<unknown>>): Paginated<KValue<unknown>> {
        return ps;
    }

}

export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(new PaginatedKvImpl());
        return await app.run(request, env);
    }
}