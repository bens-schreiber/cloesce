import {
    POST,
    WranglerEnv,
    GET,
    Inject,
    KV,
    KVModel,
    Stream
} from "cloesce/backend";
class KVNamespace { }

@WranglerEnv
class Env {
    textNamespace: KVNamespace;
    streamNamespace: KVNamespace;
    jsonNamespace: KVNamespace;
}

@KV("textNamespace")
export class TextKV extends KVModel<string> {
    @GET
    static async get(@Inject env: Env, key: string): Promise<TextKV> {
        const res = await env.textNamespace.getWithMetadata(key);
        return { ...res, key };
    }

    @POST
    static async put(@Inject env: Env, key: string, value: string): Promise<void> {
        await env.textNamespace.put(key, value);
    }

    @POST
    async delete(@Inject env: Env): Promise<void> {
        await env.textNamespace.delete(this.key);
    }
}

@KV("jsonNamespace")
export class JsonKV extends KVModel<unknown> {
    @GET
    static async get(@Inject env: Env, key: string): Promise<JsonKV> {
        const res = await env.jsonNamespace.getWithMetadata(key, { type: "json" });
        return {
            ...res, key
        }
    }

    @POST
    static async put(@Inject env: Env, key: string, json: unknown): Promise<void> {
        await env.jsonNamespace.put(key, JSON.stringify(json));
    }

    @POST
    async delete(@Inject env: Env): Promise<void> {
        await env.jsonNamespace.delete(this.key);
    }
}

@KV("streamNamespace")
export class StreamKV extends KVModel<Stream> {
    @POST
    static async put(@Inject env: Env, key: string): Promise<void> {
        await env.streamNamespace.put(key, ""); // Placeholder for stream data
    }

    @POST
    static async get(@Inject env: Env, key: string): Promise<StreamKV> {
        const res = await env.streamNamespace.getWithMetadata(key, { type: "stream" });

        // The router should remove the stream from the response as that cannot be serialized.
        return {
            ...res, key
        }
    }

    @GET
    async getStream(): Promise<Stream> {
        return this.value;
    }

    @POST
    async putStream(@Inject env: Env, stream: Stream): Promise<void> {
        await env.streamNamespace.put(this.key, stream);
    }

    @POST
    async delete(@Inject env: Env): Promise<void> {
        await env.streamNamespace.delete(this.key);
    }
}
