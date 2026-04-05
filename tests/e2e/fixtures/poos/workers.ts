import { HttpResult } from "cloesce";
import { cloesce, Env, PooA, PooAcceptYield, PooB, PooC } from "./backend.js";

export class PooAcceptYieldImpl extends PooAcceptYield.Api {
    acceptPoos(a: PooA, b: PooB, c: PooC) {
        return HttpResult.ok<void>(200);
    }

    yieldPoo() {
        return HttpResult.ok<PooC>(200, {
            a: { name: "name", major: "major" },
            b: [{ color: "color" }],
        });
    }
}


export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(new PooAcceptYieldImpl());
        return await app.run(request, env);
    }
}