import { HttpResult } from "cloesce";
import { cloesce, Env, PooAcceptYield, PooC } from "./backend.js";

export const PooAcceptYieldImpl = PooAcceptYield.impl({
    acceptPoos(a, b, c) {
        return HttpResult.ok<void>(200);
    },

    yieldPoo() {
        return HttpResult.ok<PooC>(200, {
            a: { name: "name", major: "major" },
            b: [{ color: "color" }],
        });
    },
});


export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(PooAcceptYieldImpl);
        return await app.run(request, env);
    }
}