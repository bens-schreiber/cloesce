import { PureR2Model, D1BackedModel, Env, cloesce } from "./backend.js";

export class PureR2ModelImpl extends PureR2Model.Api {
    async uploadData(self: PureR2Model.Self, e: Env, data: ReadableStream) {
        await e.bucket1.put(`path/to/data/${self.id}`, data as any);
    }

    async uploadOtherData(self: PureR2Model.Self, e: Env, data: ReadableStream) {
        await e.bucket1.put(`path/to/other/${self.id}`, data as any);
    }
}

export class D1BackedModelImpl extends D1BackedModel.Api {
    async uploadData(self: D1BackedModel.Self, e: Env, data: ReadableStream) {
        await e.bucket1.put(
            `d1Backed/${self.id}/${self.keyParam}/${self.someColumn}/${self.someOtherColumn}`,
            data as any,
        );
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
