import {
    KV, KValue, WranglerEnv, DataSource, IncludeTree, KeyParam, Model, PrimaryKey, GET
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

class KValue<T> { }
class KVNamespace { }

@WranglerEnv
export class Env {
    db: D1Database;
    namespace: KVNamespace;
    otherNamespace: KVNamespace;
}

@CRUD(["SAVE", "GET"])
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
}

@CRUD(["SAVE", "GET", "LIST"])
@Model
export class D1BackedModel {
    @PrimaryKey
    id: number;

    @KeyParam
    keyParam: string;

    someColumn: number;
    someOtherColumn: string;

    @KV("d1Backed/{id}/{keyParam}/{someColumn}/{someOtherColumn}", "namespace")
    kvData: KValue<unknown>;

    @GET
    instanceMethod(): D1BackedModel {
        if (this.kvData === undefined) {
            throw new Error("kvData is undefined");
        }

        return this;
    }
}



