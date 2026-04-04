import { NullabilityChecks, Env } from "./backend.ts";
import { HttpResult } from "cloesce";

export class NullabilityChecksImpl extends NullabilityChecks.Api {
    primitiveTypes(self: NullabilityChecks.Self, a: number | null, b: string | null) {
        return HttpResult.ok<boolean | null>(200, null);
    }

    modelTypes(self: NullabilityChecks.Self, a: NullabilityChecks.Self | null) {
        return HttpResult.ok<NullabilityChecks.Self | null>(200, null);
    }

    injectableTypes(self: NullabilityChecks.Self) {
        return HttpResult.ok<void>(200);
    }

    arrayTypes(self: NullabilityChecks.Self, a: number[] | null, b: NullabilityChecks.Self[] | null) {
        return HttpResult.ok<string[] | null>(200, null);
    }

    httpResultTypes(self: NullabilityChecks.Self) {
        return HttpResult.ok<NullabilityChecks.Self[] | null>(200, null);
    }
}
