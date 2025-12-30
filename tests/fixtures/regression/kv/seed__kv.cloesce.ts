import {
    KV, KValue, WranglerEnv, PlainOldObject, DataSource, IncludeTree
} from "cloesce/backend";
import { KVNamespace } from "@cloudflare/workers-types";
class KValue<V> { }
class KVNamespace { }

@WranglerEnv
export class Env {
    namespace: KVNamespace;
    otherNamespace: KVNamespace;
}

@KV("namespace")
export class TextValue extends KValue<string> { }

@KV("namespace")
export class JsonValue extends KValue<unknown> { }

@KV("namespace")
export class StreamValue extends KValue<ReadableStream> { }

@PlainOldObject
export class DataValue {
    id: string;
    name: string;
    age: number;
    favoriteColor: string;
}

@KV("namespace")
export class Data extends KValue<DataValue> {
    id1: string;
    id2: string;

    settings: KValue<unknown>;
}

@KV("namespace")
export class DataScientist extends KValue<unknown> {
    datasets: Data[];

    @DataSource
    static readonly withDatasets: IncludeTree<DataScientist> = {
        datasets: {
            settings: {}
        },
    };
}