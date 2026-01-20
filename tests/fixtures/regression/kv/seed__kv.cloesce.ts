import {
    KV,
    KValue,
    WranglerEnv,
    DataSource,
    IncludeTree,
    KeyParam,
    Model,
    PrimaryKey
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

@Model(["GET", "SAVE"])
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

@Model(["GET", "SAVE", "LIST"])
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
}



