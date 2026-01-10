import {
    KV,
    KValue,
    WranglerEnv,
    DataSource,
    IncludeTree,
    KeyParam,
    Model,
    PrimaryKey,
    Inject,
    DeepPartial,
    Orm,
    POST,
    CRUD
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

class KValue<T> { }
class KVNamespace { }
type Integer = number & { __kind: "Integer" };


@WranglerEnv
export class Env {
    db: D1Database;
    namespace: KVNamespace;
    otherNamespace: KVNamespace;
}

// TODO: CRUD FOR KV
@CRUD(["GET"])
@Model
export class PureKVModel {
    @KeyParam
    id: string;

    @KV("path/to/data/{id}", "namespace")
    data: KValue<unknown>;

    @KV("path/to/other/{id}", "otherNamespace")
    otherData: KValue<string>;

    @DataSource
    static readonly default: IncludeTree<PureKVModel> = {
        data: {},
        otherData: {}
    };

    @POST
    static async post(@Inject env: Env, id: string, data: unknown, otherData: string) {
        const kvKey = `path/to/data/${id}`;
        await env.namespace.put(kvKey, JSON.stringify(data));

        const otherKvKey = `path/to/other/${id}`;
        await env.otherNamespace.put(otherKvKey, JSON.stringify(otherData));
    }
}

// TODO: CRUD FOR KV
@CRUD(["GET"])
@Model
export class D1BackedModel {
    @PrimaryKey
    id: Integer;

    @KeyParam
    keyParam: string;

    someColumn: number;
    someOtherColumn: string;

    @KV("d1Backed/{id}/{keyParam}/{someColumn}/{someOtherColumn}", "namespace")
    kvData: KValue<unknown>;

    @DataSource
    static readonly default: IncludeTree<D1BackedModel> = {
        kvData: {}
    };

    @POST
    static async post(@Inject env: Env, model: DeepPartial<D1BackedModel>) {
        const orm = Orm.fromEnv(env);
        const newModel = await orm.upsert(D1BackedModel, model, {});

        // upload kvData
        const kvKey = `d1Backed/${newModel.id}/${model.keyParam}/${newModel.someColumn}/${newModel.someOtherColumn}`;
        await env.namespace.put(kvKey, JSON.stringify(model.kvData.raw));
    }
}



