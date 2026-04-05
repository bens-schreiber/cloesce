import { PureR2Model, D1BackedModel, Env, cloesce, CfReadableStream } from "./backend.js";

export class PureR2ModelImpl extends PureR2Model.Api {
    async uploadData(self: PureR2Model.Self, e: Env, data: CfReadableStream) {
        const key = PureR2Model.KeyFormat.data(self.id);
        await e.bucket1.put(key, data);
    }

    async uploadOtherData(self: PureR2Model.Self, e: Env, data: CfReadableStream) {
        const key = PureR2Model.KeyFormat.otherData(self.id);
        await e.bucket1.put(key, data as any);
    }
}

export class D1BackedModelImpl extends D1BackedModel.Api {
    async uploadData(self: D1BackedModel.Self, e: Env, data: CfReadableStream) {
        const key = D1BackedModel.KeyFormat.r2Data(self.id, self.keyParam, self.someColumn, self.someOtherColumn);
        await e.bucket1.put(key, data as any);
    }
}

export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(new PureR2ModelImpl());
        app.register(new D1BackedModelImpl());
        return await app.run(request, env);
    }
}
