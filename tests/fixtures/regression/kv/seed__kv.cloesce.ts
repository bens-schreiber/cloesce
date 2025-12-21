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

@KV("stringValueNamespace")
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
        await env.textNamespace.delete(super.key);
    }
}

@KV("jsonValueNamespace")
export class JsonKV extends KVModel<unknown> {
    @GET
    static async get(@Inject env: Env, key: string): Promise<JsonKV> {
        const res = await env.jsonNamespace.getWithMetadata(key);
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
        await env.jsonNamespace.delete(super.key);
    }
}

// @KV("streamValueNamespace")
// export class StreamKV extends KVModel<Stream> {
//     @GET
//     static async get(@Inject env: Env, key: string): Promise<StreamKV> {
//         const res = await env.streamNamespace.getWithMetadata(key);

//     }

//     @POST
//     static async put(@Inject env: Env, key: string): Promise<void> {
//         await env.streamNamespace.put(key, []); // Placeholder for stream data
//     }

//     @POST
//     async putStream(@Inject env: Env, stream: Stream): Promise<void> {
//         await env.streamNamespace.put(super.key, stream);
//     }

//     @POST
//     async delete(@Inject env: Env): Promise<void> {
//         await env.streamNamespace.delete(super.key);
//     }
// }
