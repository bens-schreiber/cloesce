import { PooAcceptYield, PooA, PooB, PooC } from "./backend.ts";
import { HttpResult } from "cloesce";

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
