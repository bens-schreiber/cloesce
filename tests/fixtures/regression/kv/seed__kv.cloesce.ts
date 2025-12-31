import {
    KV, KValue, WranglerEnv, PlainOldObject, DataSource, IncludeTree, POST, Inject, DeepPartial, GET
} from "cloesce/backend";
import { KVNamespace } from "@cloudflare/workers-types";
class KValue<V> {
    key: string;
    value: V;
    metadata: unknown;
}
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
export class Scientist {
    firstname: string;
    lastname: string;
    age: number;
}

@KV("namespace")
export class Data extends KValue<unknown> {
    key1: string;
    key2: string;

    settings: KValue<unknown>;
}

// TODO: CRUD operations
@KV("namespace")
export class DataScientist extends KValue<Scientist> {
    id: string;
    datasets: Data[];

    @DataSource
    static readonly withDatasets: IncludeTree<DataScientist> = {
        datasets: {
            settings: {}
        },
    };

    @POST
    static async post(@Inject env: Env, value: DeepPartial<DataScientist>) {
        const kv = env.namespace;

        value.id ??= crypto.randomUUID();
        const key = `DataScientist/${value.id}`;
        await kv.put(key, JSON.stringify(value));

        for (const data of value.datasets ?? []) {
            data.key1 ??= crypto.randomUUID();
            data.key2 ??= crypto.randomUUID();
            const dataKey = `${key}/datasets/${data.key1}/${data.key2}`;
            await kv.put(dataKey, JSON.stringify(data));

            if (data.settings) {
                const settingsKey = `${dataKey}/settings`;
                await kv.put(settingsKey, JSON.stringify(data.settings));
            }
        }
    }

    @GET
    static async get(@Inject env: Env, id: string): Promise<DataScientist> {
        const kv = env.namespace;
        const key = `DataScientist/${id}`;
        const value = await kv.get(key);
        if (!value) {
            throw new Error("Not found");
        }
        const scientist: DataScientist = JSON.parse(value);

        // Load datasets
        const datasets: Data[] = [];
        let cursor: string | null = null;
        do {
            const listResult = await kv.list({
                prefix: `${key}/datasets/`,
                cursor,
            });
            for (const item of listResult.keys) {
                const dataValue = await kv.get(item.name);
                if (dataValue) {
                    const data: Data = JSON.parse(dataValue);
                    // Load settings
                    const settingsKey = `${item.name}/settings`;
                    const settingsValue = await kv.get(settingsKey);
                    if (settingsValue) {
                        data.settings = JSON.parse(settingsValue);
                    }
                    datasets.push(data);
                }
            }
            cursor = listResult.cursor || null;
        } while (cursor);

        scientist.datasets = datasets;
        return scientist;
    }

    @GET
    async putMetadata(@Inject env: Env, metadata: unknown): Promise<void> {
        const kv = env.namespace;
        await kv.put(this.key, this.value, { metadata });
    }

    @GET
    async getMetadata(@Inject env: Env): Promise<unknown> {
        return this.metadata;
    }
}