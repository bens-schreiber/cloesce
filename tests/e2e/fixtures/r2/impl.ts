import { PureR2Model, D1BackedModel, Env } from "./backend.ts";
import { HttpResult } from "cloesce";

export class PureR2ModelImpl extends PureR2Model.Api {
    async uploadData(self: PureR2Model.Self, data: ReadableStream) {
        // env injected at runtime
        return HttpResult.ok<void>(200);
    }

    async uploadOtherData(self: PureR2Model.Self, data: ReadableStream) {
        return HttpResult.ok<void>(200);
    }
}

export class D1BackedModelImpl extends D1BackedModel.Api {
    async uploadData(self: D1BackedModel.Self, data: ReadableStream) {
        return HttpResult.ok<void>(200);
    }
}
