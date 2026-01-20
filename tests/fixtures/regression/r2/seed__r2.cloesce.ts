import {
    R2,
    WranglerEnv,
    DataSource,
    IncludeTree,
    KeyParam,
    Model,
    PrimaryKey,
    PUT,
    Inject
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

class R2ObjectBody { }
class R2Bucket { }
type Integer = number & { __kind: "Integer" };


@WranglerEnv
export class Env {
    db: D1Database;
    bucket1: R2Bucket;
    bucket2: R2Bucket;
}

@Model(["GET"])
export class PureR2Model {
    @KeyParam
    id: string;

    @R2("path/to/data/{id}", "bucket1")
    data: R2ObjectBody;

    @R2("path/to/other/{id}", "bucket2")
    otherData: R2ObjectBody;

    @R2("path/", "bucket1")
    allData: R2ObjectBody[];

    @PUT
    async uploadData(@Inject env: Env, stream: ReadableStream) {
        await env.bucket1.put(`path/to/data/${this.id}`, stream);
    }

    @PUT
    async uploadOtherData(@Inject env: Env, stream: ReadableStream) {
        await env.bucket2.put(`path/to/other/${this.id}`, stream);
    }

    static readonly default: IncludeTree<PureR2Model> = {
        data: {},
        otherData: {},
        allData: {}
    };
}

@Model(["GET", "SAVE", "LIST"])
export class D1BackedModel {
    @PrimaryKey
    id: Integer;

    @KeyParam
    keyParam: string;

    someColumn: number;
    someOtherColumn: string;

    @R2("d1Backed/{id}/{keyParam}/{someColumn}/{someOtherColumn}", "bucket1")
    r2Data: R2ObjectBody;

    @PUT
    async uploadData(@Inject env: Env, stream: ReadableStream) {
        await env.bucket1.put(`d1Backed/${this.id}/${this.keyParam}/${this.someColumn}/${this.someOtherColumn}`, stream);
    }

    static readonly default: IncludeTree<D1BackedModel> = {
        r2Data: {}
    };
}



